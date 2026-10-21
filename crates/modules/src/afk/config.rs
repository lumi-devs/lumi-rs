//! Per-guild AFK settings, read through the shared config cache.

use sea_orm::DatabaseConnection;

use lumi_kernel::Result;
use lumi_kernel::{Cache, module_state};

use super::constants::DEFAULT_WELCOME_TTL_SECS;
use crate::config;

const MODULE: &str = "afk";

/// Whether a welcome-back card is posted when an AFK member returns.
pub async fn welcome_enabled(
    cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<bool> {
    let values = module_state::config_cached(cache, db, guild_id, MODULE).await;
    Ok(config::boolean(&values, "welcome_back_enabled", true))
}

/// Seconds the welcome-back card stays before it (and the mention list) goes.
pub async fn welcome_ttl_secs(
    cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<u64> {
    let values = module_state::config_cached(cache, db, guild_id, MODULE).await;
    Ok(config::unsigned(
        &values,
        "welcome_ttl_seconds",
        u64::from(DEFAULT_WELCOME_TTL_SECS),
    )
    .max(1))
}

/// Whether `[AFK] ` is prepended to the nickname while AFK.
pub async fn nick_prefix_enabled(
    cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<bool> {
    let values = module_state::config_cached(cache, db, guild_id, MODULE).await;
    Ok(config::boolean(&values, "nick_prefix_enabled", true))
}
