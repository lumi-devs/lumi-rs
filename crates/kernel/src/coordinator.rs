//! Dynamic Redis shard leasing coordinator for container / Kubernetes fleets.

use std::sync::Arc;
use std::time::Duration;

use crate::cache::Cache;
use crate::config::ShardRange;
use crate::error::Result;

/// Attempt to acquire a contiguous block of unleased shards in `0..total_shards`.
pub async fn lease_shard_range(
    cache: &Cache,
    total_shards: u16,
    max_per_node: u16,
    node_id: &str,
) -> Result<Option<ShardRange>> {
    if total_shards == 0 || max_per_node == 0 {
        return Ok(None);
    }

    let mut conn = cache.redis();
    let mut start = 0;

    while start < total_shards {
        let block_end = (start + max_per_node).min(total_shards);
        let mut claimed = 0;

        for shard_id in start..block_end {
            let key = format!("lumi:shard_lease:{shard_id}");
            let res: Option<String> = redis::cmd("SET")
                .arg(&key)
                .arg(node_id)
                .arg("NX")
                .arg("EX")
                .arg(30)
                .query_async(&mut conn)
                .await?;

            if res.as_deref() == Some("OK") {
                claimed += 1;
            } else {
                break;
            }
        }

        if claimed == block_end - start {
            return Ok(Some(ShardRange {
                start,
                end: block_end.saturating_sub(1),
            }));
        }

        // Partial claim rollback to avoid dangling fragments
        for shard_id in start..(start + claimed) {
            let key = format!("lumi:shard_lease:{shard_id}");
            let _: () = redis::cmd("DEL")
                .arg(&key)
                .query_async(&mut conn)
                .await
                .unwrap_or(());
        }

        // Skip the shard that broke the run so the next block starts after it.
        start += claimed + 1;
    }

    Ok(None)
}

/// A lease may only be extended by the node whose id is stored in it.
fn lease_is_ours(current: Option<&str>, node_id: &str) -> bool {
    current == Some(node_id)
}

/// Spawns a background task that refreshes shard leases every 10 seconds.
pub fn spawn_lease_heartbeat(
    cache: Arc<Cache>,
    range: ShardRange,
    node_id: String,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(10));
        let mut conn = cache.redis();

        loop {
            interval.tick().await;

            for shard_id in range.start..=range.end {
                let key = format!("lumi:shard_lease:{shard_id}");
                // Never `SET XX`: if our lease expired and another node claimed
                // the shard, that would overwrite their ownership. Read first
                // and only extend a lease whose value is still ours.
                let current: redis::RedisResult<Option<String>> =
                    redis::cmd("GET").arg(&key).query_async(&mut conn).await;

                match current {
                    Ok(current) if lease_is_ours(current.as_deref(), &node_id) => {
                        let extended: redis::RedisResult<i64> = redis::cmd("EXPIRE")
                            .arg(&key)
                            .arg(30)
                            .query_async(&mut conn)
                            .await;
                        match extended {
                            Ok(1) => {}
                            Ok(_) => {
                                tracing::error!(
                                    shard_id,
                                    %node_id,
                                    "shard lease refresh failed: lease expired"
                                );
                            }
                            Err(err) => {
                                tracing::error!(
                                    shard_id,
                                    %node_id,
                                    error = %err,
                                    "shard lease refresh command failed"
                                );
                            }
                        }
                    }
                    Ok(_) => {
                        tracing::error!(
                            shard_id,
                            %node_id,
                            "shard lease refresh failed: lease lost or expired"
                        );
                    }
                    Err(err) => {
                        tracing::error!(
                            shard_id,
                            %node_id,
                            error = %err,
                            "shard lease refresh command failed"
                        );
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::lease_is_ours;

    #[test]
    fn only_the_owner_may_refresh_a_lease() {
        assert!(lease_is_ours(Some("node-a"), "node-a"));
        assert!(!lease_is_ours(Some("node-b"), "node-a"));
        assert!(!lease_is_ours(None, "node-a"));
    }
}
