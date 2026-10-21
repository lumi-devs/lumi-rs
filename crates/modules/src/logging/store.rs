use poise::serenity_prelude as serenity;
use sea_orm::DatabaseConnection;
use serde_json::Value;

use lumi_kernel::{Cache, Result, module_state};

const MODULE: &str = "logging";

async fn read_channel(
    cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
    key: &str,
) -> Result<Option<u64>> {
    Ok(
        module_state::config_cached_value(cache, db, guild_id, MODULE, key)
            .await
            .and_then(|v| module_state::parse_id(&v)),
    )
}

/// Whether a channel id appears in the configured ignore list. Dashboard
/// writes store snowflakes as strings, so both JSON numbers and strings count.
fn lists_channel(config: &Value, channel_id: u64) -> bool {
    config.as_array().is_some_and(|items| {
        items
            .iter()
            .any(|item| module_state::parse_id(item) == Some(channel_id))
    })
}

pub async fn is_ignored_channel(
    cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
) -> Result<bool> {
    let ignored =
        module_state::config_cached_value(cache, db, guild_id, MODULE, "ignored_channels")
            .await
            .unwrap_or_default();
    Ok(lists_channel(&ignored, channel_id))
}

/// The channel to post a given log event to, or `None` when the toggle is off
/// or no channel is configured. Resolution: event channel → group channel →
/// default channel.
pub async fn target(
    cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
    toggle_key: &str,
    event_channel_key: &str,
    group_channel_key: &str,
) -> Result<Option<serenity::GenericChannelId>> {
    // Matches lumi's `isToggleEnabled`: only an explicit `false` disables; a
    // missing value stays enabled.
    if module_state::config_cached_value(cache, db, guild_id, MODULE, toggle_key).await
        == Some(Value::Bool(false))
    {
        return Ok(None);
    }
    for key in [event_channel_key, group_channel_key, "log_channel_id"] {
        if let Some(id) = read_channel(cache, db, guild_id, key).await? {
            return Ok(Some(serenity::GenericChannelId::new(id)));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::lists_channel;
    use serde_json::json;

    #[test]
    fn ignore_list_accepts_dashboard_strings() {
        let configured = json!(["123456789012345678", 42]);
        assert!(lists_channel(&configured, 123_456_789_012_345_678));
        assert!(lists_channel(&configured, 42));
        assert!(!lists_channel(&configured, 7));
        assert!(!lists_channel(&json!(null), 42));
    }
}
