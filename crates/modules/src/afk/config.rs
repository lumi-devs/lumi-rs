//! Per-guild AFK settings, read from the shared module config table.

use sea_orm::DatabaseConnection;

use lumi_kernel::Result;
use lumi_kernel::module_state;

use super::constants::DEFAULT_WELCOME_TTL_SECS;

const MODULE: &str = "afk";

/// Whether a welcome-back card is posted when an AFK member returns.
pub async fn welcome_enabled(db: &DatabaseConnection, guild_id: u64) -> Result<bool> {
    let value = module_state::config_value(db, guild_id, MODULE, "welcome_back_enabled").await?;
    Ok(value.and_then(|value| value.as_bool()).unwrap_or(true))
}

/// Seconds the welcome-back card stays before it (and the mention list) goes.
pub async fn welcome_ttl_secs(db: &DatabaseConnection, guild_id: u64) -> Result<u64> {
    let value = module_state::config_value(db, guild_id, MODULE, "welcome_ttl_seconds").await?;
    Ok(value
        .and_then(|value| value.as_u64())
        .unwrap_or(DEFAULT_WELCOME_TTL_SECS)
        .max(1))
}

/// Whether `[AFK] ` is prepended to the nickname while AFK.
pub async fn nick_prefix_enabled(db: &DatabaseConnection, guild_id: u64) -> Result<bool> {
    let value = module_state::config_value(db, guild_id, MODULE, "nick_prefix_enabled").await?;
    Ok(value.and_then(|value| value.as_bool()).unwrap_or(true))
}
