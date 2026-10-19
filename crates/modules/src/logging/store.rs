use poise::serenity_prelude as serenity;
use sea_orm::DatabaseConnection;
use serde_json::Value;

use lumi_kernel::module_state;
use lumi_kernel::Result;

const MODULE: &str = "logging";

async fn read_channel(db: &DatabaseConnection, guild_id: u64, key: &str) -> Result<Option<u64>> {
    Ok(module_state::config_u64(db, guild_id, MODULE, key).await?)
}

/// Whether `channel_id` is in the guild's ignored-channel list.
pub async fn is_ignored_channel(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
) -> Result<bool> {
    let ignored = module_state::config_value(db, guild_id, MODULE, "ignored_channels")
        .await?
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    Ok(ignored.iter().any(|v| v.as_u64() == Some(channel_id)))
}

/// The channel to post a given log event to, or `None` when the toggle is off
/// or no channel is configured. Resolution: event channel → group channel →
/// default channel.
pub async fn target(
    db: &DatabaseConnection,
    guild_id: u64,
    toggle_key: &str,
    event_channel_key: &str,
    group_channel_key: &str,
) -> Result<Option<serenity::GenericChannelId>> {
    // Matches lumi's `isToggleEnabled`: only an explicit `false` disables; a
    // missing value stays enabled.
    if module_state::config_value(db, guild_id, MODULE, toggle_key).await? == Some(Value::Bool(false))
    {
        return Ok(None);
    }
    for key in [event_channel_key, group_channel_key, "log_channel_id"] {
        if let Some(id) = read_channel(db, guild_id, key).await? {
            return Ok(Some(serenity::GenericChannelId::new(id)));
        }
    }
    Ok(None)
}
