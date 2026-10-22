//! Health dashboard: per-shard status board with a pager.

use serenity::all::{
    ButtonStyle, ConnectionStage, CreateActionRow, CreateButton, CreateComponent,
    SeparatorSpacingSize,
};

use crate::cards::CardReply;
use crate::container::Container;
use crate::panels::{action_row_of, component_emoji};

/// Shards shown per page before the pager is needed.
pub const SHARDS_PER_PAGE: usize = 3;

/// The braille blank Discord panels use for indentation.
const SPACE: char = '\u{2800}';

/// Connection state of a shard, condensed from serenity's `ConnectionStage`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShardStatus {
    /// Shard is connecting.
    #[default]
    Connecting,
    /// Shard is connected and online.
    Online,
    /// Shard is disconnected and offline.
    Offline,
}

impl ShardStatus {
    /// Return the emoji indicator for this status.
    pub const fn dot(self) -> &'static str {
        match self {
            Self::Online => "🟢",
            Self::Connecting => "🟡",
            Self::Offline => "🔴",
        }
    }

    /// Condense a serenity gateway stage.
    pub const fn from_stage(stage: ConnectionStage) -> Self {
        match stage {
            ConnectionStage::Connected => Self::Online,
            ConnectionStage::Disconnected => Self::Offline,
            _ => Self::Connecting,
        }
    }
}

/// Per-shard connection info.
#[derive(Clone, Debug, Default)]
pub struct ShardInfo {
    /// Shard identifier.
    pub id: u64,
    /// Shard gateway ping in milliseconds.
    pub ping: f64,
    /// Current shard connection status.
    pub status: ShardStatus,
    /// Number of guilds on this shard.
    pub guilds: u64,
    /// Number of users on this shard.
    pub users: u64,
}

/// Values rendered by the ping dashboard.
#[derive(Clone, Debug, Default)]
pub struct PingData {
    /// Bot display name.
    pub bot_name: String,
    /// WebSocket gateway ping in milliseconds.
    pub ws_ping: f64,
    /// Per-shard status list.
    pub shards: Vec<ShardInfo>,
    /// Bot process uptime in milliseconds.
    pub uptime: f64,
    /// Resident set size memory in bytes.
    pub rss: u64,
    /// Total system memory in bytes.
    pub ram_total: u64,
    /// CPU usage percentage.
    pub cpu_percent: f64,
    /// Total guilds across all shards.
    pub guilds: u64,
    /// Total users across all shards.
    pub users: u64,
    /// Redis cache hit ratio percentage.
    pub redis_hit_ratio: Option<f64>,
    /// Database ping latency in milliseconds.
    pub db_latency_ms: Option<f64>,

    /// Cached channel count.
    pub channels: u64,
    /// Host operating system uptime in seconds.
    pub os_uptime_secs: u64,
    /// Bot avatar image URL.
    pub avatar_url: String,
    /// Unix seconds when this snapshot was collected.
    pub collected_at: u64,
}

fn fmt_ms(ms: Option<f64>) -> String {
    ms.filter(|n| *n >= 0.0)
        .map_or_else(|| "Analyzing…".to_string(), |n| format!("{n:.0}ms"))
}

fn fmt_mb(bytes: u64) -> String {
    const MIB: u128 = 1024 * 1024;
    let tenths = (u128::from(bytes) * 10 + (MIB / 2)) / MIB;
    format!("{}.{} MB", tenths / 10, tenths % 10)
}

fn fmt_count(count: u64) -> String {
    if count >= 1000 {
        let tenths = (count + 50) / 100;
        format!("{}.{}K", tenths / 10, tenths % 10)
    } else {
        count.to_string()
    }
}

fn ram_share(rss: u64, total: u64) -> String {
    if total == 0 {
        return "0.00%".to_string();
    }
    let total_u128 = u128::from(total);
    let bps = (u128::from(rss) * 10_000 + (total_u128 / 2)) / total_u128;
    format!("{}.{:02}%", bps / 100, bps % 100)
}

fn format_duration(ms: f64) -> String {
    use std::fmt::Write;
    let total_seconds = if ms > 0.0 {
        std::time::Duration::try_from_secs_f64(ms / 1000.0).map_or(0, |d| d.as_secs())
    } else {
        0
    };
    let days = total_seconds / 86_400;
    let hours = (total_seconds % 86_400) / 3_600;
    let minutes = (total_seconds % 3_600) / 60;
    let seconds = total_seconds % 60;
    let mut out = String::new();
    if days > 0 {
        let _ = write!(out, "{days}d");
    }
    if hours > 0 {
        if !out.is_empty() {
            out.push(' ');
        }
        let _ = write!(out, "{hours}h");
    }
    if minutes > 0 {
        if !out.is_empty() {
            out.push(' ');
        }
        let _ = write!(out, "{minutes}m");
    }
    if seconds > 0 || out.is_empty() {
        if !out.is_empty() {
            out.push(' ');
        }
        let _ = write!(out, "{seconds}s");
    }
    out
}

/// Shards to render, falling back to a dummy shard if empty.
fn effective_shards<'a>(data: &'a PingData, fallback: &'a [ShardInfo; 1]) -> &'a [ShardInfo] {
    if !data.shards.is_empty() {
        return &data.shards;
    }
    fallback
}

/// Pages of shards this dataset renders, never zero.
pub fn shard_page_count(data: &PingData) -> usize {
    let count = if data.shards.is_empty() {
        1
    } else {
        data.shards.len()
    };
    count.div_ceil(SHARDS_PER_PAGE).max(1)
}

fn shard_block(data: &PingData, shard: &ShardInfo) -> String {
    format!(
        "__{dot} **Shard [{id}]:** __\n\
         {SPACE}📡 {latency} · ⏱️ {uptime}\n\
         {SPACE}**CPU**: {cpu:.0}% . 📊 **RAM:** {ram} . **({share})**  \n\
         {SPACE}📈 **Servers:** {guilds}· 👥 **Members:** {users}",
        dot = shard.status.dot(),
        id = shard.id,
        latency = fmt_ms(Some(shard.ping)),
        uptime = format_duration(data.uptime),
        cpu = data.cpu_percent,
        ram = fmt_mb(data.rss),
        share = ram_share(data.rss, data.ram_total),
        guilds = fmt_count(shard.guilds),
        users = fmt_count(shard.users),
    )
}

/// Shard pager action row.
fn shard_pager(
    prefix: &str,
    user_id: u64,
    page: usize,
    total_pages: usize,
) -> CreateActionRow<'static> {
    let mut previous =
        CreateButton::new(format!("{prefix}:p:{}:{user_id}", page.saturating_sub(1)))
            .style(ButtonStyle::Secondary)
            .disabled(page == 0);
    if let Some(emoji) = component_emoji("\u{25c0}\u{fe0f}") {
        previous = previous.emoji(emoji);
    }
    let mut next = CreateButton::new(format!("{prefix}:n:{}:{user_id}", page.saturating_add(1)))
        .style(ButtonStyle::Secondary)
        .disabled(page + 1 >= total_pages);
    if let Some(emoji) = component_emoji("\u{25b6}\u{fe0f}") {
        next = next.emoji(emoji);
    }
    let close = CreateButton::new(format!("{prefix}:close:{user_id}"))
        .label("Close")
        .style(ButtonStyle::Danger);

    action_row_of(vec![previous, close, next])
}

/// Overview card with per-shard health and latency.
pub fn build_overview_card(data: &PingData, user_id: u64, page: usize, prefix: &str) -> CardReply {
    let fallback = [ShardInfo {
        id: 0,
        ping: data.ws_ping,
        status: if data.ws_ping >= 0.0 {
            ShardStatus::Online
        } else {
            ShardStatus::Connecting
        },
        guilds: data.guilds,
        users: data.users,
    }];
    let shards = effective_shards(data, &fallback);
    let total_pages = shard_page_count(data);
    let page = page.min(total_pages - 1);
    let mut container = Container::new()
        .text(format!(
            "__## {} Status:__\n-# Updated <t:{}:R>",
            data.bot_name, data.collected_at
        ))
        .separator(true, SeparatorSpacingSize::Small);

    for shard in shards
        .iter()
        .skip(page * SHARDS_PER_PAGE)
        .take(SHARDS_PER_PAGE)
    {
        container = container
            .text(shard_block(data, shard))
            .separator(true, SeparatorSpacingSize::Small);
    }

    container = container
        .text(format!(
            "### External Services:\n\
             {SPACE}🧠 **Cache:** {cache} \n\
             {SPACE} 🐘 **Database:** `{db}`",
            cache = data.redis_hit_ratio.map_or_else(
                || "no reads since boot".to_string(),
                |r| format!("{r:.1}% hit ratio")
            ),
            db = fmt_ms(data.db_latency_ms),
        ))
        .separator(true, SeparatorSpacingSize::Small)
        .action_row(shard_pager(prefix, user_id, page, total_pages));

    CardReply {
        components: vec![CreateComponent::Container(container.into_create())],
        ephemeral: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_shards() -> PingData {
        PingData {
            bot_name: "Lumi".to_string(),
            uptime: 109_000.0,
            rss: 168_099_430,
            ram_total: 140_000_000_000,
            cpu_percent: 0.0,
            guilds: 3,
            users: 32,
            redis_hit_ratio: Some(45.3),
            db_latency_ms: Some(3.0),
            shards: vec![
                ShardInfo {
                    id: 0,
                    ping: 424.0,
                    status: ShardStatus::Online,
                    guilds: 3,
                    users: 32,
                },
                ShardInfo {
                    id: 1,
                    ping: 338.0,
                    status: ShardStatus::Online,
                    guilds: 0,
                    users: 0,
                },
            ],
            ..PingData::default()
        }
    }

    fn texts(card: &CardReply) -> Vec<String> {
        let Ok(json) = serde_json::to_value(&card.components[0]) else {
            panic!("card serializes");
        };
        json["components"]
            .as_array()
            .map(|children| {
                children
                    .iter()
                    .filter(|child| child["type"] == 10)
                    .filter_map(|child| child["content"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn formatters_match_the_dashboard() {
        assert_eq!(fmt_ms(None), "Analyzing…");
        assert_eq!(fmt_ms(Some(-1.0)), "Analyzing…");
        assert_eq!(fmt_ms(Some(12.4)), "12ms");
        assert_eq!(fmt_mb(1024 * 1024), "1.0 MB");
        assert_eq!(fmt_count(999), "999");
        assert_eq!(fmt_count(1500), "1.5K");
        assert_eq!(format_duration(0.0), "0s");
        assert_eq!(format_duration(90_000.0), "1m 30s");
        assert_eq!(ram_share(50, 0), "0.00%");
        assert_eq!(ram_share(1_048_576, 104_857_600), "1.00%");
    }

    #[test]
    fn renders_one_block_per_shard_like_the_reference() {
        let card = build_overview_card(&two_shards(), 7, 0, "ping");
        assert!(card.ephemeral);
        let texts = texts(&card);
        assert!(texts[0].starts_with("__## Lumi Status:__\n-# Updated <t:"));
        assert!(texts[0].ends_with(":R>"));
        assert_eq!(
            texts[1],
            "__🟢 **Shard [0]:** __\n\
             \u{2800}📡 424ms · ⏱️ 1m 49s\n\
             \u{2800}**CPU**: 0% . 📊 **RAM:** 160.3 MB . **(0.12%)**  \n\
             \u{2800}📈 **Servers:** 3· 👥 **Members:** 32"
        );
        assert!(texts[2].contains("**Shard [1]:**"));
        assert!(texts[2].ends_with("**Servers:** 0· 👥 **Members:** 0"));
        assert_eq!(
            texts[3],
            "### External Services:\n\u{2800}🧠 **Cache:** 45.3% hit ratio \n\u{2800} 🐘 **Database:** `3ms`"
        );
    }

    #[test]
    fn paginates_every_three_shards() {
        let mut data = two_shards();
        data.shards = (0..7)
            .map(|id| ShardInfo {
                id,
                ping: 100.0,
                status: ShardStatus::Online,
                guilds: id,
                users: id * 2,
            })
            .collect();
        assert_eq!(shard_page_count(&data), 3);

        let last = build_overview_card(&data, 7, 2, "ping");
        let texts = texts(&last);
        assert!(texts[1].contains("**Shard [6]:**"));
        assert_eq!(texts.len(), 3);

        let first = build_overview_card(&data, 7, 0, "ping");
        let Ok(json) = serde_json::to_value(&first.components[0]) else {
            panic!("card serializes");
        };
        let buttons = json["components"]
            .as_array()
            .and_then(|children| children.iter().find(|child| child["type"] == 1))
            .and_then(|row| row["components"].as_array())
            .cloned()
            .unwrap_or_default();
        let ids: Vec<&str> = buttons
            .iter()
            .filter_map(|button| button["custom_id"].as_str())
            .collect();
        assert_eq!(ids, ["ping:p:0:7", "ping:close:7", "ping:n:1:7"]);
        assert_eq!(buttons[0]["disabled"], true);
        assert_eq!(buttons[1]["style"], 4);
        assert_eq!(buttons[2]["disabled"], false);
    }

    #[test]
    fn custom_ids_fit_discords_limit() {
        let card = build_overview_card(&two_shards(), u64::MAX, 99, "ping");
        let Ok(json) = serde_json::to_value(&card.components[0]) else {
            panic!("card serializes");
        };
        let ids = json["components"]
            .as_array()
            .and_then(|children| children.iter().find(|child| child["type"] == 1))
            .and_then(|row| row["components"].as_array())
            .cloned()
            .unwrap_or_default();
        for id in ids.iter().filter_map(|button| button["custom_id"].as_str()) {
            assert!(id.len() <= 100, "{id} is too long");
        }
    }

    #[test]
    fn out_of_range_page_clamps_to_the_last() {
        let card = build_overview_card(&two_shards(), 1, 99, "ping");
        assert_eq!(texts(&card).len(), 4);
    }
}
