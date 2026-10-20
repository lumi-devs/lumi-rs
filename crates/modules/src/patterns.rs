//! Constant regex patterns shared across modules, so a pattern is defined and
//! validated once.

/// Discord invite link; capture 1 is the invite code.
pub const INVITE_PATTERN: &str =
    r"(?i)(?:discord\.(?:gg|com/invite)|discordapp\.com/invite)/([\w-]+)";
/// URL with a host in capture 1 (mirrors lumi's `UrlRe`).
pub const URL_PATTERN: &str = r#"(?i)https?://([^\s/<>"']+)"#;
/// Custom Discord emoji token, e.g. `<:kek:123>` or `<a:cat:456>`.
pub const CUSTOM_EMOJI_PATTERN: &str = r"<a?:[a-zA-Z0-9_]+:[0-9]+>";
/// User mention or bare snowflake id in capture 1 or 2.
pub const TARGET_PATTERN: &str = r"<@!?(\d{17,20})>|(\d{17,20})";
