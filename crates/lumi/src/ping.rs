//! Health snapshot for the `ping` dashboard, ported from lumi's `PingCollect.ts`.

use serenity::all::{ConnectionStage, Context as SerenityContext};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

pub use lumi_ui::ping::{PingData, ShardInfo};

/// Collect a best-effort snapshot using `sysinfo` for all host telemetry.
pub async fn collect(addon_count: usize, ctx: &SerenityContext) -> PingData {
    let mut data = PingData::default();

    {
        let current_user = ctx.cache.current_user();
        data.bot_name = current_user.name.to_string();
        data.bot_id = current_user.id.get().to_string();
        data.avatar_url = current_user
            .static_avatar_url()
            .unwrap_or_else(|| current_user.default_avatar_url());
    }

    data.guilds = ctx.cache.guild_count() as u64;
    let mut users = 0_u64;
    let mut channels = 0_u64;
    for id in ctx.cache.guilds() {
        if let Some(guild) = ctx.cache.guild(id) {
            users += u64::from(guild.member_count.get());
            channels += guild.channels.len() as u64;
        }
    }
    data.users = users;
    data.channels = channels;
    data.modules = vec![String::new(); addon_count];

    let latency = ctx
        .runner_info
        .try_read()
        .and_then(|info| info.latency);
    let ping = latency.map_or(-1.0, |latency| latency.as_secs_f64() * 1000.0);
    data.ws_ping = ping;
    let status = ctx.runner_info.try_read().map_or_else(
        || "Disconnected".to_string(),
        |info| match info.stage {
            ConnectionStage::Connected => "Ready".to_string(),
            _ => "Disconnected".to_string(),
        },
    );
    data.shards = vec![ShardInfo {
        id: u64::from(ctx.shard_id.0),
        ping,
        status,
        sequence: 0,
    }];

    data.runtime = format!("Rust v{}", env!("CARGO_PKG_VERSION"));
    data.discord_lib = "Serenity Next + Poise".to_string();
    data.orm_lib = "SeaORM 2.0 (PostgreSQL)".to_string();
    data.gateway_node = "Unknown".to_string();

    read_host_sysinfo(&mut data).await;

    data.redis_version = "unknown".to_string();
    data.redis_frag_ratio = 1.0;

    data
}

async fn read_host_sysinfo(data: &mut PingData) {
    let s = tokio::task::spawn_blocking(|| {
        let mut sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_cpu(CpuRefreshKind::everything())
                .with_memory(MemoryRefreshKind::everything()),
        );
        sys.refresh_cpu_usage();
        sys
    })
    .await
    .unwrap_or_else(|_| System::new());

    data.platform = System::name().unwrap_or_else(|| std::env::consts::OS.to_string());
    data.arch = std::env::consts::ARCH.to_string();
    data.kernel = System::kernel_version().unwrap_or_default();
    data.os_uptime_secs = System::uptime();

    data.ram_total = s.total_memory();
    data.ram_used = s.used_memory();
    data.swap_total_kb = s.total_swap() / 1024;
    data.swap_used_kb = s.used_swap() / 1024;

    let cpus = s.cpus();
    data.cpu_cores = cpus.len() as u64;
    if let Some(first_cpu) = cpus.first() {
        data.cpu_model = first_cpu.brand().to_string();
        data.cpu_speed_mhz = first_cpu.frequency() as f64;
    }
    data.cpu_percent = s.global_cpu_usage() as f64;

    let load = System::load_average();
    data.load_avg = [load.one, load.five, load.fifteen];

    data.heap_used = data.ram_used;
    data.heap_total = data.ram_total;
    data.rss = data.ram_used;
    data.cpu_flags = "STANDARD".to_string();
}
