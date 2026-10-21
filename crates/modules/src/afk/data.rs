use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, FixedOffset, Utc};
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use serde::{Deserialize, Serialize};

use lumi_db::entities::afk_entry;
use lumi_kernel::{Cache, Result};

use super::constants::{GUILD_MAP_EMPTY_TTL, GUILD_MAP_TTL, guild_map_key};

/// One guild's AFK entry, compact enough to cache as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedAfk {
    pub reason: String,
    pub since: DateTime<FixedOffset>,
}

fn into_map(rows: Vec<afk_entry::Model>) -> HashMap<u64, CachedAfk> {
    rows.into_iter()
        .map(|row| {
            (
                row.user_id.get(),
                CachedAfk {
                    reason: row.reason,
                    since: row.since,
                },
            )
        })
        .collect()
}

async fn load_map(db: &DatabaseConnection, guild_id: u64) -> Result<HashMap<u64, CachedAfk>> {
    Ok(into_map(list(db, guild_id).await?))
}

/// Every AFK member in a guild, cached so the message path needs no query.
/// An empty map is the negative cache: nobody is AFK, and it expires sooner.
///
/// Concurrent misses share one database read.
pub async fn guild_map(
    cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<Arc<HashMap<u64, CachedAfk>>> {
    let key = guild_map_key(guild_id);
    let fetched = cache
        .get_or_fetch_json_with_ttl(&key, || async {
            let map = load_map(db, guild_id).await?;
            let ttl = guild_map_ttl(&map);
            Ok((serde_json::to_value(&map)?, ttl))
        })
        .await;

    match fetched {
        Ok(value) => match serde_json::from_value::<HashMap<u64, CachedAfk>>(value) {
            Ok(map) => Ok(Arc::new(map)),
            Err(err) => {
                tracing::warn!(error = %err, guild_id, "afk map cache held invalid JSON; reading the database");
                Ok(Arc::new(load_map(db, guild_id).await?))
            }
        },
        Err(err) => {
            tracing::warn!(error = %err, guild_id, "afk map cache failed; reading the database");
            Ok(Arc::new(load_map(db, guild_id).await?))
        }
    }
}

fn guild_map_ttl(map: &HashMap<u64, CachedAfk>) -> std::time::Duration {
    if map.is_empty() {
        GUILD_MAP_EMPTY_TTL
    } else {
        GUILD_MAP_TTL
    }
}

async fn invalidate_guild(cache: &Cache, guild_id: u64) {
    if let Err(err) = cache.invalidate(&guild_map_key(guild_id)).await {
        tracing::warn!(error = %err, guild_id, "afk map cache invalidation failed");
    }
}

pub async fn set(
    cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    reason: &str,
) -> Result<()> {
    invalidate_guild(cache, guild_id).await;
    let row = afk_entry::ActiveModel {
        user_id: Set(lumi_db::ids::UserId::from(user_id)),
        guild_id: Set(lumi_db::ids::GuildId::from(guild_id)),
        reason: Set(reason.to_string()),
        since: Set(Utc::now().fixed_offset()),
    };
    afk_entry::Entity::insert(row)
        .on_conflict(
            OnConflict::columns([afk_entry::Column::UserId, afk_entry::Column::GuildId])
                .update_columns([afk_entry::Column::Reason, afk_entry::Column::Since])
                .to_owned(),
        )
        .exec(db)
        .await?;
    invalidate_guild(cache, guild_id).await;
    Ok(())
}

pub async fn get(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
) -> Result<Option<afk_entry::Model>> {
    Ok(afk_entry::Entity::find()
        .filter(afk_entry::Column::GuildId.eq(guild_id as i64))
        .filter(afk_entry::Column::UserId.eq(user_id as i64))
        .one(db)
        .await?)
}

pub async fn clear(
    cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
) -> Result<bool> {
    invalidate_guild(cache, guild_id).await;
    let result = afk_entry::Entity::delete_many()
        .filter(afk_entry::Column::GuildId.eq(guild_id as i64))
        .filter(afk_entry::Column::UserId.eq(user_id as i64))
        .exec(db)
        .await?;
    invalidate_guild(cache, guild_id).await;
    Ok(result.rows_affected > 0)
}

pub async fn list(db: &DatabaseConnection, guild_id: u64) -> Result<Vec<afk_entry::Model>> {
    Ok(afk_entry::Entity::find()
        .filter(afk_entry::Column::GuildId.eq(guild_id as i64))
        .order_by_asc(afk_entry::Column::Since)
        .all(db)
        .await?)
}

/// Counted in the database instead of loading every row.
pub async fn count(db: &DatabaseConnection, guild_id: u64) -> Result<u64> {
    Ok(afk_entry::Entity::find()
        .filter(afk_entry::Column::GuildId.eq(guild_id as i64))
        .count(db)
        .await?)
}

/// Collapse a multi-line reason to one line, defaulting to `AFK`.
pub fn sanitize_reason(reason: &str) -> String {
    let joined = reason
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let collapsed = joined.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return "AFK".to_string();
    }
    crate::shared::truncate_chars(&collapsed, super::constants::MAX_REASON)
}

#[cfg(test)]
mod tests {
    use super::{CachedAfk, GUILD_MAP_EMPTY_TTL, GUILD_MAP_TTL, guild_map_ttl, sanitize_reason};
    use std::collections::HashMap;

    #[test]
    fn sanitize_reason_collapses_and_defaults() {
        assert_eq!(sanitize_reason("  out\n\n to   lunch "), "out to lunch");
        assert_eq!(sanitize_reason("\n  \n"), "AFK");
        assert_eq!(sanitize_reason(&"x".repeat(200)).len(), 100);
    }

    #[test]
    fn empty_map_is_negative_cached_for_the_short_ttl() {
        let empty = HashMap::new();
        assert_eq!(guild_map_ttl(&empty), GUILD_MAP_EMPTY_TTL);

        let mut map = HashMap::new();
        map.insert(
            1,
            CachedAfk {
                reason: "lunch".into(),
                since: chrono::Utc::now().fixed_offset(),
            },
        );
        assert_eq!(guild_map_ttl(&map), GUILD_MAP_TTL);
    }
}
