use poise::serenity_prelude as serenity;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde_json::Value;

use lumi_db::entities::guild_config;
use lumi_kernel::Result;

const MODULE: &str = "logging";

pub async fn get(
    db: &DatabaseConnection,
    guild_id: u64,
    key: &str,
) -> Result<Option<Value>> {
    Ok(guild_config::Entity::find()
        .filter(guild_config::Column::GuildId.eq(guild_id as i64))
        .filter(guild_config::Column::Module.eq(MODULE))
        .filter(guild_config::Column::Key.eq(key))
        .one(db)
        .await?
        .map(|model| model.value))
}

pub async fn set(
    db: &DatabaseConnection,
    guild_id: u64,
    key: &str,
    value: Value,
) -> Result<()> {
    guild_config::Entity::insert(guild_config::ActiveModel {
        guild_id: Set(guild_id as i64),
        module: Set(MODULE.to_string()),
        key: Set(key.to_string()),
        value: Set(value),
    })
    .on_conflict(
        OnConflict::columns([
            guild_config::Column::GuildId,
            guild_config::Column::Module,
            guild_config::Column::Key,
        ])
        .update_columns([guild_config::Column::Value])
        .to_owned(),
    )
    .exec(db)
    .await?;
    Ok(())
}

async fn read_channel(db: &DatabaseConnection, guild_id: u64, key: &str) -> Result<Option<u64>> {
    Ok(get(db, guild_id, key).await?.and_then(|v| v.as_u64()))
}

/// Whether `channel_id` is in the guild's ignored-channel list.
pub async fn is_ignored_channel(
    db: &DatabaseConnection,
    guild_id: u64,
    channel_id: u64,
) -> Result<bool> {
    let ignored = get(db, guild_id, "ignored_channels")
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
    if get(db, guild_id, toggle_key).await? == Some(Value::Bool(false)) {
        return Ok(None);
    }
    for key in [event_channel_key, group_channel_key, "log_channel_id"] {
        if let Some(id) = read_channel(db, guild_id, key).await? {
            return Ok(Some(serenity::GenericChannelId::new(id)));
        }
    }
    Ok(None)
}
