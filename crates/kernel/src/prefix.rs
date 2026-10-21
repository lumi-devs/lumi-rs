//! Cached per-guild command prefix.
//!
//! Reads are on the hot message path, so a guild's resolved prefix (including
//! the global default) is cached as one string.

use std::time::Duration;

use sea_orm::DatabaseConnection;
use serde_json::Value;

use crate::{Cache, Result};

/// Prefix entries expire well after a dashboard edit; writes invalidate them.
const PREFIX_TTL: Duration = Duration::from_secs(600);

pub const PREFIX_NAMESPACE: &str = "lumi:guild:prefix:";

fn key(guild_id: u64) -> String {
    format!("{PREFIX_NAMESPACE}{guild_id}")
}

pub async fn get(cache: &Cache, db: &DatabaseConnection, guild_id: u64) -> Result<String> {
    let key = key(guild_id);
    let cached = match cache.get_json_ttl(&key).await {
        Ok(Some(value)) => value.as_str().map(str::to_owned),
        Ok(None) => None,
        Err(err) => {
            tracing::warn!(error = %err, guild_id, "prefix cache read failed; reading the database");
            return Ok(lumi_db::get_prefix(db, Some(guild_id)).await?);
        }
    };
    if let Some(prefix) = cached {
        return Ok(prefix);
    }

    let prefix = lumi_db::get_prefix(db, Some(guild_id)).await?;
    if let Err(err) = cache
        .set_json_ttl(&key, &Value::String(prefix.clone()), PREFIX_TTL)
        .await
    {
        tracing::warn!(error = %err, guild_id, "prefix cache write failed");
    }
    Ok(prefix)
}

pub async fn invalidate(cache: &Cache, guild_id: u64) -> Result<()> {
    if let Err(err) = cache.invalidate(&key(guild_id)).await {
        tracing::warn!(error = %err, guild_id, "prefix cache invalidation failed");
    }
    Ok(())
}
