//! Health snapshot for the `ping` dashboard.
//!
//! Host figures cover the whole box; RAM and CPU cover this process alone, so
//! the dashboard reports the bot rather than the machine running it. Guilds and
//! members are split per shard with Discord's own shard math, which is why a
//! shard serving nothing reads zero instead of repeating the global count.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use poise::serenity_prelude as serenity;
use serenity::all::{Context as SerenityContext, MessageFlags};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

use lumi_kernel::Services;
use lumi_ui::ping::{PingData, ShardInfo, ShardStatus, build_overview_card};

/// How long a rendered dashboard keeps refreshing itself, and how often.
const LIVE_WINDOW: Duration = Duration::from_secs(60);
const LIVE_INTERVAL: Duration = Duration::from_secs(10);

/// Linux reports process CPU time in `USER_HZ` ticks, which the kernel defines
/// as 100 per second on every architecture.
const USER_HZ: f64 = 100.0;

/// The gateway's shard snapshot: what this process runs, and the fleet total the
/// shard math needs.
#[derive(Clone, Debug, Default)]
pub struct ShardSnapshot {
    pub shards: Vec<ShardInfo>,
    pub total: u64,
}

/// Live shard state, supplied by the gateway because only it holds the shard
/// manager. Absent in the API/worker tiers, which run no shards.
static SHARDS: OnceLock<Arc<dyn Fn() -> ShardSnapshot + Send + Sync>> = OnceLock::new();

/// Hand the gateway's shard snapshot over once the client is built.
pub fn track_shards(source: impl Fn() -> ShardSnapshot + Send + Sync + 'static) {
    let _ = SHARDS.set(Arc::new(source));
}

/// The dashboard page each viewer is on, forgotten when their message dies.
static PAGES: OnceLock<Mutex<HashMap<u64, usize>>> = OnceLock::new();

fn pages() -> &'static Mutex<HashMap<u64, usize>> {
    PAGES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The page a viewer is looking at.
#[must_use]
pub fn page_of(user_id: u64) -> usize {
    pages()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(&user_id)
        .copied()
        .unwrap_or(0)
}

/// Render the first page and remember it as this viewer's current one.
pub fn reset_page(user_id: u64) {
    pages()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(user_id, 0);
}

/// Step a viewer's page by `delta`, clamped to the pages that exist.
pub fn move_page(user_id: u64, delta: i64, total_pages: usize) -> usize {
    let mut pages = pages().lock().unwrap_or_else(PoisonError::into_inner);
    let current = pages.get(&user_id).copied().unwrap_or(0);
    let step = usize::try_from(delta.unsigned_abs()).unwrap_or(0);
    let next = if delta.is_negative() {
        current.saturating_sub(step)
    } else {
        current.saturating_add(step)
    }
    .min(total_pages.saturating_sub(1));
    pages.insert(user_id, next);
    next
}

/// Forget a viewer's page once their dashboard is gone.
pub fn forget_page(user_id: u64) {
    pages()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .remove(&user_id);
}

/// This process's memory, runtime and CPU figures.
#[derive(Default)]
struct ProcessSnapshot {
    rss: u64,
    ram_total: u64,
    uptime_secs: u64,
    cpu_time_ms: u64,
}

/// `utime + stime` of this process, in clock ticks, from `/proc/self/stat`.
///
/// The `comm` field can contain spaces and parentheses, so parsing starts after
/// the final `)`: `state` is field 3, `utime`/`stime` are fields 14 and 15.
fn process_cpu_ticks() -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let mut fields = stat.rsplit_once(')')?.1.split_whitespace();
    let utime = fields.nth(11)?.parse::<u64>().ok()?;
    let stime = fields.next()?.parse::<u64>().ok()?;
    Some(utime + stime)
}

fn process_snapshot() -> ProcessSnapshot {
    let pid = Pid::from_u32(std::process::id());
    let mut system = System::new();
    system.refresh_memory();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    let process = system.process(pid);
    ProcessSnapshot {
        rss: process.map_or(0, sysinfo::Process::memory),
        ram_total: system.total_memory(),
        uptime_secs: process.map_or(0, sysinfo::Process::run_time),
        cpu_time_ms: process_cpu_ticks()
            .map_or(0, |ticks| (ticks as f64 / USER_HZ * 1000.0) as u64),
    }
}

/// Every shard this process is connected to, or this context's own shard.
fn shard_infos(ctx: &SerenityContext) -> (Vec<ShardInfo>, u64) {
    let snapshot = SHARDS
        .get()
        .map_or_else(ShardSnapshot::default, |source| source());
    if !snapshot.shards.is_empty() {
        return (snapshot.shards, snapshot.total.max(1));
    }

    let status = ctx
        .runner_info
        .try_read()
        .map_or(ShardStatus::Connecting, |info| {
            ShardStatus::from_stage(info.stage)
        });
    (
        vec![ShardInfo {
            id: u64::from(ctx.shard_id.get()),
            ping: ws_ping(ctx),
            status,
            guilds: 0,
            users: 0,
        }],
        1,
    )
}

/// Collect a best-effort snapshot of the bot, its backends and the host.
pub async fn collect(services: &Services, ctx: &SerenityContext) -> PingData {
    let process_before = tokio::task::spawn_blocking(process_snapshot)
        .await
        .unwrap_or_default();
    let started = Instant::now();

    let (db_latency_ms, redis_hit_ratio) = tokio::join!(
        async {
            let started = Instant::now();
            services.db.ping().await.ok()?;
            Some(started.elapsed().as_secs_f64() * 1000.0)
        },
        async { services.cache.hit_ratio().await.unwrap_or(None) },
    );

    let process_after = tokio::task::spawn_blocking(process_snapshot)
        .await
        .unwrap_or_default();
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    // `/proc` CPU time is summed over every core the process used, so a busy
    // multi-threaded process exceeds 100%; normalise to whole-machine percent.
    let cores = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get) as f64;
    let cpu_percent = if elapsed_ms > 0.0 {
        process_after
            .cpu_time_ms
            .saturating_sub(process_before.cpu_time_ms) as f64
            / elapsed_ms
            * 100.0
            / cores
    } else {
        0.0
    };

    let (mut shards, shard_total) = shard_infos(ctx);

    let mut users = 0_u64;
    let mut channels = 0_u64;
    let mut served: HashMap<u64, (u64, u64)> =
        shards.iter().map(|shard| (shard.id, (0, 0))).collect();
    for id in ctx.cache.guilds() {
        let Some(guild) = ctx.cache.guild(id) else {
            continue;
        };
        let members = u64::from(guild.member_count.get());
        users += members;
        channels += guild.channels.len() as u64;
        if let Some(counts) = served.get_mut(&((guild.id.get() >> 22) % shard_total)) {
            *counts = (counts.0 + 1, counts.1 + members);
        }
    }
    for shard in &mut shards {
        if let Some((guilds, members)) = served.get(&shard.id) {
            shard.guilds = *guilds;
            shard.users = *members;
        }
    }

    let current_user = ctx.cache.current_user();
    let avatar_url = current_user
        .static_avatar_url()
        .unwrap_or_else(|| current_user.default_avatar_url());

    PingData {
        bot_name: current_user.name.to_string(),
        ws_ping: ws_ping(ctx),
        shards,
        uptime: process_after.uptime_secs as f64 * 1000.0,
        rss: process_after.rss,
        ram_total: process_after.ram_total,
        cpu_percent,
        guilds: ctx.cache.guild_count() as u64,
        users,
        redis_hit_ratio,
        db_latency_ms,

        channels,
        os_uptime_secs: System::uptime(),
        avatar_url,
    }
}

/// Keep a rendered dashboard refreshing itself for a minute, the way lumi's
/// `Ping.ts` does, stopping as soon as the message is gone.
pub fn spawn_live_updates(
    services: Arc<Services>,
    gateway: SerenityContext,
    token: String,
    user_id: u64,
) {
    tokio::spawn(async move {
        let deadline = Instant::now() + LIVE_WINDOW;
        while Instant::now() < deadline {
            tokio::time::sleep(LIVE_INTERVAL).await;
            let data = collect(&services, &gateway).await;
            let card = build_overview_card(
                &data,
                user_id,
                page_of(user_id),
                crate::interactions::PING_PREFIX,
            );
            let edit = serenity::EditMessage::new()
                .components(card.components)
                .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                .allowed_mentions(serenity::CreateAllowedMentions::new());
            if let Err(e) = gateway
                .http
                .edit_original_interaction_response(&token, &edit, Vec::new())
                .await
            {
                tracing::debug!(error = %e, user_id, "ping dashboard refresh stopped");
                forget_page(user_id);
                break;
            }
        }
    });
}

/// The gateway's reported round-trip latency in milliseconds, `-1` when unknown.
fn ws_ping(ctx: &SerenityContext) -> f64 {
    ctx.runner_info
        .try_read()
        .and_then(|info| info.latency)
        .map_or(-1.0, |latency| latency.as_secs_f64() * 1000.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_snapshot_reads_this_process_and_the_host() {
        let snapshot = process_snapshot();
        assert!(snapshot.rss > 0, "process RSS must be read");
        assert!(
            snapshot.ram_total > snapshot.rss,
            "host memory must be refreshed, not left at zero"
        );
    }

    #[test]
    fn pages_move_within_bounds() {
        assert_eq!(move_page(1, -1, 3), 0);
        assert_eq!(move_page(1, 1, 3), 1);
        assert_eq!(move_page(1, 5, 3), 2);
        assert_eq!(page_of(1), 2);
        forget_page(1);
        assert_eq!(page_of(1), 0);
    }
}
