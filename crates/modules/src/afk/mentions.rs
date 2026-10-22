//! Who mentioned an AFK member, kept as a capped Redis list per user.
//!
//! Redis is the right home for this: the list is hot, short-lived, read only
//! from the welcome-back flow, and losing it costs nothing.

use serde::{Deserialize, Serialize};

use lumi_kernel::{Cache, Result};
use redis::AsyncCommands;

use super::constants::{MAX_MENTIONS, MENTIONS_TTL_SECS, mentions_key};

/// One message that mentioned an AFK member.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mention {
    pub author_id: u64,
    pub author_name: String,
    pub channel_id: u64,
    pub message_id: u64,
    pub ts: i64,
}

/// Push one record per AFK user, keeping only the newest [`MAX_MENTIONS`].
pub async fn add_batch(cache: &Cache, guild_id: u64, batch: &[(u64, Mention)]) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let mut conn = cache.redis();
    let mut pipe = redis::pipe();
    for (user_id, mention) in batch {
        let key = mentions_key(guild_id, *user_id);
        pipe.lpush(&key, serde_json::to_string(mention)?)
            .ignore()
            .ltrim(
                &key,
                0,
                isize::try_from(MAX_MENTIONS).unwrap_or(isize::MAX) - 1,
            )
            .ignore()
            .expire(&key, MENTIONS_TTL_SECS)
            .ignore();
    }
    let _: () = pipe.query_async(&mut conn).await?;
    Ok(())
}

/// Newest-first mention records for one user.
pub async fn list(cache: &Cache, guild_id: u64, user_id: u64) -> Result<Vec<Mention>> {
    let mut conn = cache.redis();
    let raw: Vec<String> = conn.lrange(mentions_key(guild_id, user_id), 0, -1).await?;
    Ok(raw
        .iter()
        .filter_map(|item| serde_json::from_str(item).ok())
        .collect())
}

/// Drop every mention record for one user.
pub async fn clear(cache: &Cache, guild_id: u64, user_id: u64) -> Result<()> {
    let mut conn = cache.redis();
    let _: i64 = conn.del(mentions_key(guild_id, user_id)).await?;
    Ok(())
}
