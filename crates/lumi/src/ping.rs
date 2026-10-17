//! Health snapshot for the `ping` dashboard, ported from lumi's `PingCollect.ts`.

use serenity::all::{ConnectionStage, Context as SerenityContext};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};

/// Per-shard connection info.
#[derive(Clone, Debug)]
pub struct ShardInfo {
    pub id: u64,
    pub ping: f64,
    pub status: String,
    pub sequence: u64,
}

/// Every value the ping dashboard can render. Fields missing a real source
/// default to zero/empty (see [`collect`]).
#[derive(Clone, Debug, Default)]
pub struct PingData {
    pub round_trip: Option<f64>,

    pub ws_ping: f64,
    pub loop_lag_ms: f64,
    pub jitter_ms: f64,
    pub shards: Vec<ShardInfo>,
    pub active_handles: u64,
    pub active_requests: u64,
    pub gateway_node: String,
    pub identifies: u64,
    pub resumes: u64,

    pub heap_used: u64,
    pub heap_total: u64,
    pub rss: u64,
    pub external: u64,
    pub array_buffers: u64,
    pub runtime: String,

    pub kernel: String,
    pub platform: String,
    pub arch: String,
    pub cpu_model: String,
    pub cpu_cores: u64,
    pub cpu_speed_mhz: f64,
    pub load_avg: [f64; 3],
    pub ram_used: u64,
    pub ram_total: u64,
    pub swap_used_kb: u64,
    pub swap_total_kb: u64,
    pub os_uptime_secs: u64,
    pub ctx_switch_vol: u64,
    pub ctx_switch_nonvol: u64,
    pub disk_read_bytes: u64,
    pub disk_write_bytes: u64,
    pub thermal_celsius: Option<f64>,
    pub cpu_percent: f64,
    pub cpu_flags: String,
    pub io_wait: u64,

    pub prisma_ms: Option<f64>,
    pub db_size: Option<String>,
    pub db_uptime_secs: Option<u64>,
    pub table_sizes: Vec<(String, u64)>,
    pub db_commits: Option<u64>,
    pub db_rollbacks: Option<u64>,
    pub tx_rate: f64,

    pub redis_read_ms: Option<f64>,
    pub redis_write_ms: Option<f64>,
    pub redis_version: String,
    pub redis_uptime_secs: u64,
    pub redis_mem_used_bytes: u64,
    pub redis_mem_peak_bytes: u64,
    pub redis_frag_ratio: f64,
    pub redis_hit_ratio: f64,
    pub redis_hits: u64,
    pub redis_misses: u64,
    pub redis_evicted: u64,
    pub redis_clients: u64,
    pub redis_total_keys: u64,

    pub uptime: f64,
    pub guilds: u64,
    pub users: u64,
    pub channels: u64,
    pub modules: Vec<String>,
    pub avatar_url: String,
    pub bot_name: String,
    pub bot_id: String,
    pub session_command_count: u64,
    pub dep_count: u64,
    pub code_lines: u64,
    pub commands_per_sec: f64,
    pub messages_per_min: f64,

    pub djs_version: String,
    pub prisma_version: String,
}

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
    data.djs_version = "0.12".to_string();
    data.prisma_version = "sea-orm".to_string();
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
