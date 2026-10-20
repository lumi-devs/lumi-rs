//! The `ping` health dashboard: a per-shard status board with a pager.
//!
//! The layout mirrors the reference dashboard exactly: a `__## <bot> Status:__`
//! header, one block per shard (three per page) separated by dividers, an
//! external-services block, and a `◀️ · Close · ▶️` pager.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::time::{SystemTime, UNIX_EPOCH};

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
    #[default]
    Connecting,
    Online,
    Offline,
}

impl ShardStatus {
    /// Status dot rendered next to the shard id.
    #[must_use]
    pub const fn dot(self) -> &'static str {
        match self {
            Self::Online => "🟢",
            Self::Connecting => "🟡",
            Self::Offline => "🔴",
        }
    }

    /// Condense a serenity gateway stage.
    #[must_use]
    pub const fn from_stage(stage: ConnectionStage) -> Self {
        match stage {
            ConnectionStage::Connected => Self::Online,
            ConnectionStage::Disconnected => Self::Offline,
            _ => Self::Connecting,
        }
    }
}

/// Per-shard connection info plus the slice of the bot that shard serves.
#[derive(Clone, Debug, Default)]
pub struct ShardInfo {
    pub id: u64,
    pub ping: f64,
    pub status: ShardStatus,
    pub guilds: u64,
    pub users: u64,
}

/// Every value the ping dashboard renders.
#[derive(Clone, Debug, Default)]
pub struct PingData {
    pub bot_name: String,
    pub ws_ping: f64,
    pub shards: Vec<ShardInfo>,
    pub uptime: f64,
    pub rss: u64,
    pub ram_total: u64,
    pub cpu_percent: f64,
    pub guilds: u64,
    pub users: u64,
    pub redis_hit_ratio: Option<f64>,
    pub db_latency_ms: Option<f64>,

    /// Instance counters and host uptime shared with `about`.
    pub channels: u64,
    pub os_uptime_secs: u64,
    pub avatar_url: String,
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

fn fmt_count(count: u64) -> String {
    if count >= 1000 {
        format!("{:.1}K", count as f64 / 1000.0)
    } else {
        count.to_string()
    }
}

/// Share of the machine's RAM this process holds, as the dashboard's percentage.
fn ram_share(rss: u64, total: u64) -> String {
    if total == 0 {
        return "0.00%".to_string();
    }
    format!("{:.2}%", rss as f64 / total as f64 * 100.0)
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

/// The shards to render: the live snapshot, or a stand-in for tiers that run none.
fn effective_shards(data: &PingData) -> Vec<ShardInfo> {
    if !data.shards.is_empty() {
        return data.shards.clone();
    }
    vec![ShardInfo {
        id: 0,
        ping: data.ws_ping,
        status: if data.ws_ping >= 0.0 {
            ShardStatus::Online
        } else {
            ShardStatus::Connecting
        },
        guilds: data.guilds,
        users: data.users,
    }]
}

/// Pages of shards this dataset renders, never zero.
#[must_use]
pub fn shard_page_count(data: &PingData) -> usize {
    effective_shards(data)
        .len()
        .div_ceil(SHARDS_PER_PAGE)
        .max(1)
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

/// The `◀️ · Close · ▶️` pager, disabled at either end, keyed to the viewer.
fn shard_pager(
    prefix: &str,
    user_id: u64,
    page: usize,
    total_pages: usize,
) -> CreateActionRow<'static> {
    let mut previous = CreateButton::new(format!(
        "{prefix}:p:{}:{user_id}",
        page.saturating_sub(1)
    ))
    .style(ButtonStyle::Secondary)
    .disabled(page == 0);
    if let Some(emoji) = component_emoji("\u{25c0}\u{fe0f}") {
        previous = previous.emoji(emoji);
    }
    let mut next = CreateButton::new(format!(
        "{prefix}:n:{}:{user_id}",
        page.saturating_add(1)
    ))
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

/// The dashboard page: header, three shards, external services and the pager.
#[must_use]
pub fn build_overview_card(data: &PingData, user_id: u64, page: usize, prefix: &str) -> CardReply {
    let shards = effective_shards(data);
    let total_pages = shard_page_count(data);
    let page = page.min(total_pages - 1);
    let updated = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());

    let mut container = Container::new()
        .text(format!(
            "__## {} Status:__\n-# Updated <t:{updated}:R>",
            data.bot_name
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
            cache = data
                .redis_hit_ratio
                .map_or_else(|| "no reads since boot".to_string(), |r| format!("{r:.1}% hit ratio")),
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
