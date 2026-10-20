//! AFK limits, Redis key shapes and cooldown windows.

use std::time::Duration;

/// Nickname prefix applied while AFK.
pub const NICK_PREFIX: &str = "[AFK] ";
/// Longest accepted AFK reason, in characters.
pub const MAX_REASON: usize = 100;
/// Discord's nickname limit, in characters.
pub const MAX_NICKNAME: usize = 32;

/// Newest mention records kept per AFK user.
pub const MAX_MENTIONS: i64 = 25;
/// How long mention records survive, in seconds.
pub const MENTIONS_TTL_SECS: i64 = 24 * 60 * 60;

/// One "is AFK" notice per channel, at most once per window.
pub const MENTION_COOLDOWN_MS: u64 = 5_000;
/// One welcome-back message per user + channel, at most once per window.
pub const WELCOME_COOLDOWN_MS: u64 = 5_000;
/// Grace period after `/afk` before messages can clear it again.
pub const REMOVAL_COOLDOWN_MS: u64 = 2_000;
/// Nickname edits are cheap but rate limited; one per second per user.
pub const NICK_COOLDOWN_MS: u64 = 1_000;

/// Default lifetime of the welcome-back message, in seconds.
pub const DEFAULT_WELCOME_TTL_SECS: u64 = 20;
/// Lifetime of an "is AFK" notice.
pub const NOTICE_TTL: Duration = Duration::from_secs(600);

pub fn mention_cooldown_key(channel_id: u64) -> String {
    format!("lumi:afk:cd:mention:{channel_id}")
}

pub fn welcome_cooldown_key(channel_id: u64, user_id: u64) -> String {
    format!("lumi:afk:cd:welcome:{channel_id}:{user_id}")
}

pub fn removal_cooldown_key(guild_id: u64, user_id: u64) -> String {
    format!("lumi:afk:cd:removal:{guild_id}:{user_id}")
}

pub fn nick_cooldown_key(user_id: u64) -> String {
    format!("lumi:afk:cd:nick:{user_id}")
}

pub fn mentions_key(guild_id: u64, user_id: u64) -> String {
    format!("lumi:afk:mentions:{guild_id}:{user_id}")
}
