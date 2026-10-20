//! Per-guild AFK settings, cached as one JSON object per guild.

use sea_orm::DatabaseConnection;
use serde_json::{Map, Value};

use lumi_kernel::Result;
use lumi_kernel::{Cache, module_state};

use super::constants::{CONFIG_CACHE_TTL, DEFAULT_WELCOME_TTL_SECS, config_key};

const MODULE: &str = "afk";

async fn load(
    cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
) -> Map<String, Value> {
    let key = config_key(guild_id);
    match cache.get_json_ttl(&key).await {
        Ok(Some(value)) => match serde_json::from_value::<Map<String, Value>>(value) {
            Ok(config) => return config,
            Err(err) => tracing::warn!(error = %err, guild_id, "afk config cache held invalid JSON"),
        },
        Ok(None) => {}
        Err(err) => {
            tracing::warn!(error = %err, guild_id, "afk config cache read failed; reading the database");
            return module_state::get_config(db, guild_id, MODULE).await;
        }
    }

    let config = module_state::get_config(db, guild_id, MODULE).await;
    if let Err(err) = cache
        .set_json_ttl(&key, &Value::Object(config.clone()), CONFIG_CACHE_TTL)
        .await
    {
        tracing::warn!(error = %err, guild_id, "afk config cache write failed");
    }
    config
}

/// Whether a welcome-back card is posted when an AFK member returns.
pub async fn welcome_enabled(
    cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<bool> {
    let config = load(cache, db, guild_id).await;
    Ok(config
        .get("welcome_back_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true))
}

/// Seconds the welcome-back card stays before it (and the mention list) goes.
pub async fn welcome_ttl_secs(
    cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<u64> {
    let config = load(cache, db, guild_id).await;
    Ok(config
        .get("welcome_ttl_seconds")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_WELCOME_TTL_SECS)
        .max(1))
}

/// Whether `[AFK] ` is prepended to the nickname while AFK.
pub async fn nick_prefix_enabled(
    cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<bool> {
    let config = load(cache, db, guild_id).await;
    Ok(config
        .get("nick_prefix_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true))
}
