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
    // Concurrent cold misses share one database read (and one global fallback).
    match cache
        .get_or_fetch_json(&key, PREFIX_TTL, || async {
            Ok(Value::String(
                lumi_db::get_prefix(db, Some(guild_id)).await?,
            ))
        })
        .await
    {
        Ok(Value::String(prefix)) => Ok(prefix),
        Ok(_) => Ok(lumi_db::get_prefix(db, Some(guild_id)).await?),
        Err(err) => {
            tracing::warn!(error = %err, guild_id, "prefix cache failed; reading the database");
            Ok(lumi_db::get_prefix(db, Some(guild_id)).await?)
        }
    }
}

pub async fn invalidate(cache: &Cache, guild_id: u64) -> Result<()> {
    if let Err(err) = cache.invalidate(&key(guild_id)).await {
        tracing::warn!(error = %err, guild_id, "prefix cache invalidation failed");
    }
    Ok(())
}
