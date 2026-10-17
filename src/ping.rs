//! Health snapshot for the `ping` dashboard, ported from lumi's `PingCollect.ts`.

use serenity::all::{ConnectionStage, Context as SerenityContext};

use crate::services::Services;

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

/// Collect a best-effort snapshot. Gateway/cache values are real; host values
/// come from `/proc`; everything without a wired source is left at its default.
pub async fn collect(services: &Services, ctx: &SerenityContext) -> PingData {
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
    data.modules = vec![String::new(); services.addons.count()];

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

    read_host(&mut data).await;

    // shortcut: Redis/Postgres probes are not wired yet; the Services DB/Redis
    // handles land next, at which point redis_*/prisma_*/db_* become real.
    data.redis_version = "unknown".to_string();
    data.redis_frag_ratio = 1.0;

    data
}

async fn read_host(data: &mut PingData) {
    data.platform = std::env::consts::OS.to_string();
    data.arch = std::env::consts::ARCH.to_string();

    if let Some(mem) = read_async("/proc/meminfo").await {
        let total_kb = proc_value(&mem, "MemTotal:");
        let mut free_kb = proc_value(&mem, "MemAvailable:");
        if free_kb == 0 {
            free_kb = proc_value(&mem, "MemFree:");
        }
        data.ram_total = total_kb * 1024;
        data.ram_used = total_kb.saturating_sub(free_kb) * 1024;
        let swap_total_kb = proc_value(&mem, "SwapTotal:");
        let swap_free_kb = proc_value(&mem, "SwapFree:");
        data.swap_total_kb = swap_total_kb;
        data.swap_used_kb = swap_total_kb.saturating_sub(swap_free_kb);
    }

    if let Some(status) = read_async("/proc/self/status").await {
        data.ctx_switch_vol = proc_value(&status, "voluntary_ctxt_switches:");
        data.ctx_switch_nonvol = proc_value(&status, "nonvoluntary_ctxt_switches:");
        data.rss = proc_value(&status, "VmRSS:") * 1024;
    }

    if let Some(io) = read_async("/proc/self/io").await {
        data.disk_read_bytes = proc_value(&io, "rchar:");
        data.disk_write_bytes = proc_value(&io, "wchar:");
    }

    if let Some(cpuinfo) = read_async("/proc/cpuinfo").await {
        data.cpu_model = cpuinfo
            .lines()
            .find_map(|line| line.strip_prefix("model name"))
            .and_then(|line| line.split_once(':'))
            .map_or_else(|| "Unknown".to_string(), |(_, value)| value.trim().to_string());
        data.cpu_cores = cpuinfo
            .lines()
            .filter(|line| line.starts_with("processor"))
            .count() as u64;
        data.cpu_speed_mhz = cpuinfo
            .lines()
            .find_map(|line| line.strip_prefix("cpu MHz"))
            .and_then(|line| line.split_once(':'))
            .and_then(|(_, value)| value.trim().parse::<f64>().ok())
            .unwrap_or(0.0);
        data.cpu_flags = cpu_flags(&cpuinfo);
    }

    if let Some(stat) = read_async("/proc/stat").await {
        data.io_wait = stat
            .lines()
            .find(|line| line.starts_with("cpu "))
            .and_then(|line| line.split_whitespace().nth(5))
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
    }

    if let Some(load) = read_async("/proc/loadavg").await {
        let values: Vec<f64> = load
            .split_whitespace()
            .take(3)
            .filter_map(|value| value.parse().ok())
            .collect();
        if values.len() == 3 {
            data.load_avg = [values[0], values[1], values[2]];
        }
    }

    if let Some(uptime) = read_async("/proc/uptime").await {
        data.os_uptime_secs = uptime
            .split_whitespace()
            .next()
            .and_then(|value| value.parse::<f64>().ok())
            .map_or(0, |seconds| seconds as u64);
    }

    if let Some(kernel) = read_async("/proc/sys/kernel/osrelease").await {
        data.kernel = kernel.trim().to_string();
    }

    data.thermal_celsius = read_async("/sys/class/thermal/thermal_zone0/temp")
        .await
        .and_then(|raw| raw.trim().parse::<i64>().ok())
        .map(|milli| milli as f64 / 1000.0);

    data.heap_used = data.rss;
    data.heap_total = data.rss;
}

async fn read_async(path: &str) -> Option<String> {
    tokio::fs::read_to_string(path).await.ok()
}

fn proc_value(text: &str, key: &str) -> u64 {
    text.lines()
        .find_map(|line| line.strip_prefix(key))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0)
}

fn cpu_flags(cpuinfo: &str) -> String {
    let Some(line) = cpuinfo.lines().find_map(|line| line.strip_prefix("flags")) else {
        return "STANDARD".to_string();
    };
    let Some((_, rest)) = line.split_once(':') else {
        return "STANDARD".to_string();
    };
    rest.split_whitespace()
        .filter(|flag| matches!(*flag, "avx2" | "sse4_2" | "aes" | "rdseed"))
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}
