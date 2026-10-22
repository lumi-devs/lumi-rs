use poise::serenity_prelude as serenity;
use sea_orm::DatabaseConnection;
use serde_json::Value;

use lumi_kernel::{Cache, Result, module_state};

const MODULE: &str = "logging";

/// Whether `channel_id` is in the guild's ignored-channel list.
pub async fn is_ignored_channel(
    cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
) -> Result<bool> {
    let ignored =
        module_state::config_cached_value(cache, db, guild_id, MODULE, "ignored_channels").await;
    Ok(ignored
        .as_ref()
        .and_then(Value::as_array)
        .is_some_and(|channels| {
            channels
                .iter()
                .any(|value| module_state::parse_id(value) == Some(channel_id))
        }))
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
    // One cache read + one decode for the whole lookup instead of four.
    let config = module_state::config_cached(cache, db, guild_id, MODULE).await;

    // Matches lumi's `isToggleEnabled`: only an explicit `false` disables; a
    // missing value stays enabled.
    if config.get(toggle_key) == Some(&Value::Bool(false)) {
        return Ok(None);
    }
    for key in [event_channel_key, group_channel_key, "log_channel_id"] {
        if let Some(id) = config.get(key).and_then(module_state::parse_id) {
            return Ok(Some(serenity::GenericChannelId::new(id)));
        }
    }
    Ok(None)
}
