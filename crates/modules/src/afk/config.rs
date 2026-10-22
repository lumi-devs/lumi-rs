//! Per-guild AFK settings, read through the shared module-config cache.

use sea_orm::DatabaseConnection;
use serde_json::{Map, Value};

use lumi_kernel::{Cache, module_state};

use super::constants::DEFAULT_WELCOME_TTL_SECS;

const MODULE: &str = "afk";

async fn load(cache: &Cache, db: &DatabaseConnection, guild_id: u64) -> Map<String, Value> {
    // One cache key per guild+module: sharing `module_state` keeps dashboard
    // writes and invalidation from having to know about a module-local copy.
    module_state::config_cached(cache, db, guild_id, MODULE).await
}

/// Whether a welcome-back card is posted when an AFK member returns.
pub async fn welcome_enabled(cache: &Cache, db: &DatabaseConnection, guild_id: u64) -> bool {
    let config = load(cache, db, guild_id).await;
    config
        .get("welcome_back_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

/// Seconds the welcome-back card stays before it (and the mention list) goes.
pub async fn welcome_ttl_secs(cache: &Cache, db: &DatabaseConnection, guild_id: u64) -> u64 {
    let config = load(cache, db, guild_id).await;
    config
        .get("welcome_ttl_seconds")
        .and_then(Value::as_u64)
        .unwrap_or(DEFAULT_WELCOME_TTL_SECS)
        .max(1)
}

/// Whether `[AFK] ` is prepended to the nickname while AFK.
pub async fn nick_prefix_enabled(cache: &Cache, db: &DatabaseConnection, guild_id: u64) -> bool {
    let config = load(cache, db, guild_id).await;
    config
        .get("nick_prefix_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true)
}
