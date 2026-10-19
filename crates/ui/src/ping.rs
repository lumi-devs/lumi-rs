//! Faithful port of lumi's `src/modules/onboarding/ui/PingCards.ts`.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use serenity::all::{
    ButtonStyle, CreateActionRow, CreateButton, CreateComponent, CreateSection,
    CreateSectionComponent, SeparatorSpacingSize,
};

use crate::cards::CardReply;
use crate::container::{Container, section_with_thumbnail, text_display};
use crate::panels::{action_row_of, component_emoji};

/// Per-shard connection info.
#[derive(Clone, Debug, Default)]
pub struct ShardInfo {
    pub id: u64,
    pub ping: f64,
    pub status: String,
    pub sequence: u64,
}

/// Every value the ping dashboard can render.
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
    pub db_latency_ms: Option<f64>,
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
    pub discord_lib: String,
    pub orm_lib: String,
}

/// Custom-id prefix shared by every ping button.
pub const PING_ID_PREFIX: &str = "ping";

/// Overview view pseudo-category.
pub const OVERVIEW: &str = "overview";

const DEFAULT_AVATAR: &str = "https://cdn.discordapp.com/embed/avatars/0.png";
const SPACE: char = '\u{2800}';

/// Build a ping button custom id: `ping:{category}:{user_id}`.
#[must_use]
pub fn ping_custom_id(category: &str, user_id: u64) -> String {
    format!("{PING_ID_PREFIX}:{category}:{user_id}")
}

/// A detail card category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PingCategory {
    Gateway,
    Engine,
    Host,
    Postgres,
    Redis,
    Bot,
}

impl PingCategory {
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "gateway" => Some(Self::Gateway),
            "engine" => Some(Self::Engine),
            "host" => Some(Self::Host),
            "postgres" => Some(Self::Postgres),
            "redis" => Some(Self::Redis),
            "bot" => Some(Self::Bot),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Gateway => "gateway",
            Self::Engine => "engine",
            Self::Host => "host",
            Self::Postgres => "postgres",
            Self::Redis => "redis",
            Self::Bot => "bot",
        }
    }
}

fn fmt_ms(ms: Option<f64>) -> String {
    match ms {
        Some(n) if n >= 0.0 => format!("{n:.0}ms"),
        _ => "Analyzing…".to_string(),
    }
}

fn fmt_mb(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
}

fn fmt_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} GB", bytes as f64 / 1024.0 / 1024.0)
    } else if bytes >= 1024 {
        format!("{:.1} MB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

fn fmt_count(count: u64) -> String {
    if count >= 1000 {
        format!("{:.1}K", count as f64 / 1000.0)
    } else {
        count.to_string()
    }
}

fn format_number(n: u64) -> String {
    let digits = n.to_string();
    let len = digits.len();
    let mut out = String::with_capacity(len + len / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

fn format_duration(ms: f64) -> String {
    let total_seconds = (ms / 1000.0).floor().max(0.0) as u64;
    let days = total_seconds / 86400;
    let hours = (total_seconds % 86400) / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    let mut parts: Vec<String> = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 {
        parts.push(format!("{minutes}m"));
    }
    if seconds > 0 || parts.is_empty() {
        parts.push(format!("{seconds}s"));
    }
    parts.join(" ")
}

fn executive_section(title: &str, fields: &[(String, String)]) -> String {
    let header = format!("{SPACE}__{title}__:");
    let lines = fields
        .iter()
        .map(|(label, value)| format!("{SPACE}{SPACE}**{label}:** {value}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("{header}\n{lines}")
}

/// `## {botName}{sub}\n-# {55× ⠀}` with a thumbnail accessory.
fn header(data: &PingData, subtitle: Option<&str>) -> CreateSection<'static> {
    let sub = subtitle.map_or_else(String::new, |s| format!("\n*{s}*"));
    let anchor = format!("-# {}", SPACE.to_string().repeat(55));
    let content = format!("## {}{sub}\n{anchor}", data.bot_name);
    let avatar = if data.avatar_url.is_empty() {
        DEFAULT_AVATAR
    } else {
        &data.avatar_url
    };
    section_with_thumbnail(
        vec![CreateSectionComponent::TextDisplay(text_display(content))],
        avatar.to_string(),
    )
}

fn detail_card(subtitle: &str, data: &PingData) -> Container {
    Container::new()
        .section(header(data, Some(subtitle)))
        .separator(true, SeparatorSpacingSize::Small)
}

/// Pagination row: ◀️ (disabled) · Close · ▶️ (disabled when single page).
fn overview_nav_row(user_id: u64, page: usize, total: usize) -> CreateActionRow<'static> {
    let prev = CreateButton::new(format!("{PING_ID_PREFIX}:p_-1:{user_id}:{page}"))
        .emoji('◀')
        .style(ButtonStyle::Secondary)
        .disabled(page == 0);
    let close = CreateButton::new(ping_custom_id("close", user_id))
        .label("Close")
        .style(ButtonStyle::Danger);
    let next = CreateButton::new(format!("{PING_ID_PREFIX}:p_1:{user_id}:{page}"))
        .emoji('▶')
        .style(ButtonStyle::Secondary)
        .disabled(page + 1 >= total);
    action_row_of(vec![prev, close, next])
}

#[must_use]
pub fn build_overview_card_paged(data: &PingData, page: usize, user_id: u64) -> CardReply {
    const PAGE_SIZE: usize = 5;
    let fallback = [ShardInfo {
        id: 0,
        ping: data.ws_ping,
        status: String::new(),
        sequence: 0,
    }];
    let shards: &[ShardInfo] = if data.shards.is_empty() {
        &fallback
    } else {
        &data.shards
    };

    let total_pages = shards.len().div_ceil(PAGE_SIZE).max(1);
    let page = page.min(total_pages - 1);
    let start = page.saturating_mul(PAGE_SIZE).min(shards.len());
    let end = start.saturating_add(PAGE_SIZE).min(shards.len());
    let page_shards = &shards[start..end];

    let ram_mb = data.rss as f64 / 1024.0 / 1024.0;
    let ram_pct = if data.ram_total > 0 {
        data.rss as f64 / data.ram_total as f64 * 100.0
    } else {
        0.0
    };

    let mut c = Container::new()
        .text(format!("__## {} Status:__", data.bot_name));

    for shard in page_shards {
        c = c.separator(true, SeparatorSpacingSize::Small);
        let shard_text = format!(
            "__🟢 **Shard [{}]:** __\n{SPACE}📡 {} · ⏱️ {}\n{SPACE}**CPU**: {:.0}% . 📊 **RAM:** {:.1} MB . **({:.2}%)**  \n{SPACE}📈 **Servers:** {}· 👥 **Members:** {}",
            shard.id,
            fmt_ms(Some(shard.ping)),
            format_duration(data.uptime),
            data.cpu_percent,
            ram_mb,
            ram_pct,
            fmt_count(data.guilds),
            fmt_count(data.users),
        );
        c = c.text(shard_text);
    }

    c = c.separator(true, SeparatorSpacingSize::Small);
    c = c.text(format!(
        "### External Services:\n{SPACE}🧠 **Cache:** {:.1}% hit ratio \n{SPACE} 🐘 **Database:** `{}`",
        data.redis_hit_ratio,
        fmt_ms(data.db_latency_ms),
    ));
    c = c.separator(true, SeparatorSpacingSize::Small);
    c = c.action_row(overview_nav_row(user_id, page, total_pages));

    container_reply(c)
}

fn build_gateway_card(data: &PingData) -> Container {
    let mut card = detail_card("📡 Gateway & Connection Status", data);

    let node = if data.gateway_node == "Unknown" {
        "Analyzing…".to_string()
    } else {
        data.gateway_node.clone()
    };

    let body = [
        executive_section(
            "Connection Health",
            &[
                ("Average Latency".to_string(), fmt_ms(Some(data.ws_ping))),
                ("Jitter".to_string(), format!("±{:.0}ms", data.jitter_ms)),
            ],
        ),
        executive_section(
            "Session Details",
            &[
                (
                    "Initial Handshakes".to_string(),
                    format!("{} times", data.identifies),
                ),
                (
                    "Successful Resumes".to_string(),
                    format!("{} times", data.resumes),
                ),
                ("Regional Gateway".to_string(), format!("{node} Hub")),
            ],
        ),
        executive_section(
            "Traffic Activity",
            &[
                (
                    "Incoming Messages".to_string(),
                    format!("{:.0} msg/min", data.messages_per_min),
                ),
                (
                    "Active Requests".to_string(),
                    format!("{} concurrent", data.active_requests),
                ),
            ],
        ),
    ]
    .join("\n");
    card = card.text(body);

    if !data.shards.is_empty() {
        card = card.separator(true, SeparatorSpacingSize::Small);
        card = card.text(format!("{SPACE}__Active Shards__:"));
        let lines = data
            .shards
            .iter()
            .map(|s| {
                format!(
                    "{SPACE}{SPACE}**Shard {}:** {} | {} | Sequence {}",
                    s.id,
                    fmt_ms(Some(s.ping)),
                    s.status,
                    s.sequence
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        card = card.text(lines);
    }

    card
}

fn build_engine_card(data: &PingData) -> Container {
    let heap_pct = if data.heap_total > 0 {
        data.heap_used as f64 / data.heap_total as f64 * 100.0
    } else {
        0.0
    };
    let body = [
        executive_section(
            "Memory Allocation",
            &[
                (
                    "JS Heap Used".to_string(),
                    format!("{} ({heap_pct:.1}%)", fmt_mb(data.heap_used)),
                ),
                ("External Heap".to_string(), fmt_mb(data.external)),
                ("Total RSS Cost".to_string(), fmt_mb(data.rss)),
            ],
        ),
        executive_section(
            "Execution Lag",
            &[
                (
                    "Event Loop Lag".to_string(),
                    format!("{:.0}ms", data.loop_lag_ms),
                ),
                (
                    "Timer Handles".to_string(),
                    format!("{} Active", data.active_handles),
                ),
            ],
        ),
        executive_section(
            "Core Environment",
            &[
                ("Engine Runtime".to_string(), data.runtime.clone()),
                (
                    "Library Stack".to_string(),
                    format!("{} | {}", data.discord_lib, data.orm_lib),
                ),
            ],
        ),
    ]
    .join("\n");
    detail_card("🏎️ Performance & Memory", data).text(body)
}

fn build_host_card(data: &PingData) -> Container {
    let temp = data
        .thermal_celsius
        .map_or_else(|| "N/A".to_string(), |c| format!("{c:.1}°C"));
    let body = [
        executive_section(
            "Processor Topology",
            &[
                ("CPU Model".to_string(), data.cpu_model.clone()),
                (
                    "Logic Cores".to_string(),
                    format!("{} Cores ({})", data.cpu_cores, data.arch),
                ),
            ],
        ),
        executive_section(
            "Memory & Swap",
            &[
                (
                    "System RAM".to_string(),
                    format!(
                        "{:.2} GB / {:.2} GB",
                        data.ram_used as f64 / 1024.0 / 1024.0 / 1024.0,
                        data.ram_total as f64 / 1024.0 / 1024.0 / 1024.0
                    ),
                ),
                (
                    "Swap Used".to_string(),
                    format!("{:.0} MB", data.swap_used_kb as f64 / 1024.0),
                ),
            ],
        ),
        executive_section(
            "Host Environment",
            &[
                (
                    "OS / Kernel".to_string(),
                    format!("{} {}", data.platform, data.kernel),
                ),
                ("CPU Temp".to_string(), temp),
            ],
        ),
    ]
    .join("\n");
    detail_card("🖥️ System Infrastructure", data).text(body)
}

fn build_postgres_card(data: &PingData) -> Container {
    let mut card = detail_card("🐘 Database Health", data);

    let body = [
        executive_section(
            "Performance",
            &[
                (
                    "Transaction Speed".to_string(),
                    format!("{:.1} ops/sec", data.tx_rate),
                ),
                ("Query Response".to_string(), fmt_ms(data.db_latency_ms)),
            ],
        ),
        executive_section(
            "Database Details",
            &[
                (
                    "Database Size".to_string(),
                    data.db_size.clone().unwrap_or_else(|| "N/A".to_string()),
                ),
                (
                    "Database Uptime".to_string(),
                    data.db_uptime_secs
                        .map_or_else(|| "N/A".to_string(), |s| format_duration(s as f64 * 1000.0)),
                ),
            ],
        ),
    ]
    .join("\n");
    card = card.text(body);

    if !data.table_sizes.is_empty() {
        card = card.separator(true, SeparatorSpacingSize::Small);
        card = card.text(format!("{SPACE}__📊 Table Storage__:"));
        let lines = data
            .table_sizes
            .iter()
            .map(|(name, bytes)| format!("{SPACE}{SPACE}**{name}:** {}", fmt_bytes(*bytes)))
            .collect::<Vec<_>>()
            .join("\n");
        card = card.text(lines);
    }

    card
}

fn build_redis_card(data: &PingData) -> Container {
    let body = [
        executive_section(
            "Memory Usage",
            &[
                (
                    "Current Usage".to_string(),
                    fmt_mb(data.redis_mem_used_bytes),
                ),
                (
                    "Peak Usage".to_string(),
                    fmt_mb(data.redis_mem_peak_bytes),
                ),
            ],
        ),
        executive_section(
            "Cache Hits & Efficiency",
            &[
                (
                    "Hit Ratio".to_string(),
                    format!("{:.2}%", data.redis_hit_ratio),
                ),
                (
                    "Tracked Keys".to_string(),
                    format_number(data.redis_total_keys),
                ),
            ],
        ),
        executive_section(
            "Connection",
            &[
                (
                    "Redis Version".to_string(),
                    format!("v{}", data.redis_version),
                ),
                (
                    "Active Clients".to_string(),
                    format!("{} connected", data.redis_clients),
                ),
            ],
        ),
    ]
    .join("\n");
    detail_card("🧠 Cache Performance", data).text(body)
}

fn build_bot_card(data: &PingData) -> Container {
    let mem_per_guild = if data.guilds > 0 {
        format!(
            "{:.2}",
            data.rss as f64 / 1024.0 / 1024.0 / data.guilds as f64
        )
    } else {
        "0".to_string()
    };
    let body = [
        executive_section(
            "Interaction Analytics",
            &[
                (
                    "Command Rate".to_string(),
                    format!("{:.0}/sec", data.commands_per_sec),
                ),
                (
                    "Message Traffic".to_string(),
                    format!("{:.0}/min", data.messages_per_min),
                ),
            ],
        ),
        executive_section(
            "Efficiency Metrics",
            &[
                ("Memory/Guild".to_string(), format!("{mem_per_guild} MB")),
                ("System RSS".to_string(), fmt_mb(data.rss)),
            ],
        ),
        executive_section(
            "Software Architecture",
            &[
                (
                    "Source Lines".to_string(),
                    format!("{} TS", format_number(data.code_lines)),
                ),
                (
                    "Feature Modules".to_string(),
                    format!("{} Loaded", data.modules.len()),
                ),
            ],
        ),
    ]
    .join("\n");
    detail_card("🤖 Bot System Summary", data).text(body)
}

/// Detail card for `category`, ending with a `Back to Overview` button.
#[must_use]
pub fn build_detail_card(category: PingCategory, data: &PingData, user_id: u64) -> CardReply {
    let card = match category {
        PingCategory::Gateway => build_gateway_card(data),
        PingCategory::Engine => build_engine_card(data),
        PingCategory::Host => build_host_card(data),
        PingCategory::Postgres => build_postgres_card(data),
        PingCategory::Redis => build_redis_card(data),
        PingCategory::Bot => build_bot_card(data),
    };
    container_reply(
        card.separator(true, SeparatorSpacingSize::Small)
            .action_row(back_row(user_id)),
    )
}

fn back_row(user_id: u64) -> CreateActionRow<'static> {
    let button = CreateButton::new(ping_custom_id(OVERVIEW, user_id))
        .label("Back to Overview")
        .style(ButtonStyle::Secondary);
    let button = match component_emoji("⬅️") {
        Some(emoji) => button.emoji(emoji),
        None => button,
    };
    action_row_of(vec![button])
}

fn container_reply(container: Container) -> CardReply {
    CardReply {
        components: vec![CreateComponent::Container(container.into_create())],
        ephemeral: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatters_match_ts() {
        assert_eq!(fmt_ms(None), "Analyzing…");
        assert_eq!(fmt_ms(Some(-1.0)), "Analyzing…");
        assert_eq!(fmt_ms(Some(12.4)), "12ms");
        assert_eq!(fmt_mb(1024 * 1024), "1.0 MB");
        assert_eq!(fmt_bytes(2048), "2.0 MB");
        assert_eq!(fmt_bytes(1024 * 1024), "1.0 GB");
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(fmt_count(999), "999");
        assert_eq!(fmt_count(1500), "1.5K");
        assert_eq!(format_number(1_234_567), "1,234,567");
        assert_eq!(format_duration(0.0), "0s");
        assert_eq!(format_duration(90_000.0), "1m 30s");
    }

    #[test]
    fn custom_id_round_trips() {
        assert_eq!(ping_custom_id(OVERVIEW, 42), "ping:overview:42");
        assert_eq!(
            ping_custom_id(PingCategory::Postgres.as_str(), 7),
            "ping:postgres:7"
        );
    }

    #[test]
    fn categories_round_trip() {
        for category in [
            PingCategory::Gateway,
            PingCategory::Engine,
            PingCategory::Host,
            PingCategory::Postgres,
            PingCategory::Redis,
            PingCategory::Bot,
        ] {
            assert_eq!(PingCategory::from_id(category.as_str()), Some(category));
        }
        assert_eq!(PingCategory::from_id("nope"), None);
    }
}
