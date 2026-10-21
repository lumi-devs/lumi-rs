//! Small helpers shared by feature modules.
//!
//! Kept deliberately narrow: guild resolution, cooldown claiming, number and
//! duration formatting, character-safe truncation and page math.

use poise::serenity_prelude as serenity;
use serenity::all::GuildId;

use lumi_kernel::Cache;
use lumi_kernel::bot::{Context, Error};
use lumi_kernel::module::{ConfigField, ConfigFieldKind};
use serde_json::{Value, json};

/// One dashboard config field. `group` is the dashboard section label.
pub fn config_field(
    key: &str,
    label: &str,
    description: &str,
    kind: ConfigFieldKind,
    default: Value,
    group: Option<&str>,
) -> ConfigField {
    ConfigField {
        key: key.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        kind,
        default,
        subfields: Vec::new(),
        group: group.map(str::to_string),
    }
}

pub fn string_field(
    key: &str,
    label: &str,
    description: &str,
    default: &str,
    group: Option<&str>,
) -> ConfigField {
    config_field(
        key,
        label,
        description,
        ConfigFieldKind::String,
        json!(default),
        group,
    )
}

pub fn int_field(
    key: &str,
    label: &str,
    description: &str,
    default: i64,
    group: Option<&str>,
) -> ConfigField {
    config_field(
        key,
        label,
        description,
        ConfigFieldKind::Integer,
        json!(default),
        group,
    )
}

pub fn bool_field(
    key: &str,
    label: &str,
    description: &str,
    default: bool,
    group: Option<&str>,
) -> ConfigField {
    config_field(
        key,
        label,
        description,
        ConfigFieldKind::Boolean,
        json!(default),
        group,
    )
}

pub fn channel_field(
    key: &str,
    label: &str,
    description: &str,
    group: Option<&str>,
) -> ConfigField {
    config_field(
        key,
        label,
        description,
        ConfigFieldKind::Channel,
        Value::Null,
        group,
    )
}

pub fn string_array_field(
    key: &str,
    label: &str,
    description: &str,
    group: Option<&str>,
) -> ConfigField {
    config_field(
        key,
        label,
        description,
        ConfigFieldKind::StringArray,
        json!([]),
        group,
    )
}

pub fn enum_field(
    key: &str,
    label: &str,
    description: &str,
    options: &[&str],
    default: &str,
    group: Option<&str>,
) -> ConfigField {
    config_field(
        key,
        label,
        description,
        ConfigFieldKind::Enum(options.iter().map(|o| (*o).to_string()).collect()),
        json!(default),
        group,
    )
}

/// Guild id for a guild-only command, as a typed error instead of a panic.
pub fn require_guild(ctx: Context<'_>) -> Result<GuildId, Error> {
    ctx.guild_id()
        .ok_or_else(|| anyhow::anyhow!("this command can only be used in a server"))
}

/// Claim `key` for `ttl_ms`. `Ok(None)` means proceed; `Ok(Some(ms))` is the
/// remaining cooldown in milliseconds.
pub async fn claim_cooldown(cache: &Cache, key: &str, ttl_ms: u64) -> Result<Option<i64>, Error> {
    if cache.claim_cooldown(key, ttl_ms).await? {
        Ok(None)
    } else {
        Ok(Some(cache.cooldown_remaining_ms(key).await?))
    }
}

/// Stable Redis key for a per-user cooldown in a module.
pub fn cooldown_key(scope: &str, guild_id: u64, user_id: u64) -> String {
    format!("lumi:{scope}:{guild_id}:{user_id}")
}

/// Milliseconds rounded up to whole seconds, clamped at zero.
pub fn secs_ceil(ms: i64) -> i64 {
    ms.max(0).saturating_add(999) / 1000
}

/// Human-readable duration, clamped at zero, at most the two largest units
/// (`90_000` -> `1d 1h`, `3_725` -> `1h 2m`, `65` -> `1m 5s`, `5` -> `5s`).
pub fn format_seconds(seconds: i64) -> String {
    let total = seconds.max(0);
    let (days, rest) = (total / 86_400, total % 86_400);
    let (hours, rest) = (rest / 3_600, rest % 3_600);
    let (minutes, secs) = (rest / 60, rest % 60);
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

/// `1,234,567` / `-42`.
pub fn group_digits(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

pub fn truncate_chars(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// Cut to at most `max` characters, appending `…` when the text was shortened.
pub fn ellipsize(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

/// Clamped page math for a fixed page size: `(page, total_pages, offset)`.
pub fn page_bounds(total_items: u64, per_page: u64, requested: usize) -> (usize, usize, u64) {
    let per_page = per_page.max(1);
    let total_pages = total_items.div_ceil(per_page).max(1) as usize;
    let page = requested.min(total_pages - 1);
    (page, total_pages, page as u64 * per_page)
}

#[cfg(test)]
mod tests {
    use super::{ellipsize, format_seconds, group_digits, page_bounds, truncate_chars};

    #[test]
    fn groups_thousands() {
        assert_eq!(group_digits(0), "0");
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1000), "1,000");
        assert_eq!(group_digits(1_234_567), "1,234,567");
        assert_eq!(group_digits(-42_000), "-42,000");
    }

    #[test]
    fn formats_durations() {
        assert_eq!(format_seconds(-1), "0s");
        assert_eq!(format_seconds(5), "5s");
        assert_eq!(format_seconds(65), "1m 5s");
        assert_eq!(format_seconds(3_725), "1h 2m");
        assert_eq!(format_seconds(90_000), "1d 1h");
    }

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate_chars("héllo", 3), "hél");
        assert_eq!(ellipsize("héllo", 4), "hél\u{2026}");
        assert_eq!(ellipsize("ok", 10), "ok");
    }

    #[test]
    fn clamps_pages() {
        assert_eq!(page_bounds(0, 10, 0), (0, 1, 0));
        assert_eq!(page_bounds(25, 10, 1), (1, 3, 10));
        assert_eq!(page_bounds(25, 10, 99), (2, 3, 20));
        assert_eq!(page_bounds(5, 10, 0), (0, 1, 0));
    }
}
