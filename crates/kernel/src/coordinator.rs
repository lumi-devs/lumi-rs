//! Dynamic Redis shard leasing coordinator for container / Kubernetes fleets.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::watch;

use crate::cache::Cache;
use crate::config::ShardRange;
use crate::error::Result;

/// Lease lifetime in Redis. A process that cannot renew before this elapses
/// must stop serving, otherwise two nodes can own the same shard.
pub const LEASE_TTL: Duration = Duration::from_secs(30);

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

/// Delete a lease only when this node still owns it. A lease that expired and
/// was re-acquired by another node must never be released by the old owner.
const RELEASE_SCRIPT: &str = "if redis.call('get', KEYS[1]) == ARGV[1] \
                              then return redis.call('del', KEYS[1]) else return 0 end";

/// Renew only while this node still owns the key. Plain `SET XX` would let a
/// node whose lease expired steal the shard back from the current owner.
const RENEW_SCRIPT: &str = "if redis.call('get', KEYS[1]) == ARGV[1] \
                             then return redis.call('set', KEYS[1], ARGV[1], 'EX', ARGV[2]) \
                             else return false end";

/// Renew `shard_id` if (and only if) `node_id` still owns it.
async fn renew_one(
    conn: &mut redis::aio::ConnectionManager,
    shard_id: u16,
    node_id: &str,
) -> redis::RedisResult<Option<String>> {
    let script = redis::Script::new(RENEW_SCRIPT);
    let key = format!("lumi:shard_lease:{shard_id}");
    script
        .key(key)
        .arg(node_id)
        .arg(LEASE_TTL.as_secs())
        .invoke_async(conn)
        .await
}

async fn release_one(
    conn: &mut redis::aio::ConnectionManager,
    shard_id: u16,
    node_id: &str,
) -> Result<i64> {
    let script = redis::Script::new(RELEASE_SCRIPT);
    let key = format!("lumi:shard_lease:{shard_id}");
    let released = script.key(key).arg(node_id).invoke_async(conn).await?;
    metrics::gauge!("lumi.shard.lease.owned", "shard" => shard_id.to_string()).set(0.0);
    Ok(released)
}

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
                .arg(LEASE_TTL.as_secs())
                .query_async(&mut conn)
                .await?;

            if res.as_deref() == Some("OK") {
                claimed += 1;
                metrics::gauge!("lumi.shard.lease.owned", "shard" => shard_id.to_string()).set(1.0);
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

        // Partial claim rollback; compare-and-delete keeps it fencing-safe.
        for shard_id in start..(start + claimed) {
            if let Err(error) = release_one(&mut conn, shard_id, node_id).await {
                tracing::warn!(
                    shard_id,
                    %node_id,
                    %error,
                    "failed to roll back partial shard lease"
                );
            }
        }

        // Skip the shard that broke the run so the next block starts after it.
        start += claimed + 1;
    }

    Ok(None)
}

/// Release every shard lease this node still owns.
///
/// The compare-and-delete script makes release fencing-safe: a lease that has
/// already expired and been claimed by another node is left untouched.
pub async fn release_shard_range(cache: &Cache, range: ShardRange, node_id: &str) -> Result<()> {
    let mut conn = cache.redis();
    let mut first_error = None;
    for shard_id in range.start..=range.end {
        if let Err(error) = release_one(&mut conn, shard_id, node_id).await {
            tracing::warn!(shard_id, %node_id, %error, "failed to release shard lease");
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// Outcome of one heartbeat renewal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseRefresh {
    Renewed,
    Lost,
    Transient,
}

fn classify_refresh(result: &redis::RedisResult<Option<String>>) -> LeaseRefresh {
    match result {
        Ok(Some(value)) if value == "OK" => LeaseRefresh::Renewed,
        Ok(_) => LeaseRefresh::Lost,
        Err(_) => LeaseRefresh::Transient,
    }
}

/// Fencing decision for one renewal result. A lost lease fences immediately;
/// a transport error only fences once the TTL has provably run out.
fn should_fence(outcome: LeaseRefresh, last_renewed: Instant, now: Instant, ttl: Duration) -> bool {
    match outcome {
        LeaseRefresh::Renewed => false,
        LeaseRefresh::Lost => true,
        LeaseRefresh::Transient => now.saturating_duration_since(last_renewed) >= ttl,
    }
}

/// Running lease heartbeat. `lost` flips to `true` when this process no longer
/// owns the range and must stop serving.
#[derive(Debug)]
pub struct LeaseHeartbeat {
    lost: watch::Receiver<bool>,
    handle: tokio::task::JoinHandle<()>,
}

impl LeaseHeartbeat {
    #[must_use]
    pub fn lost(&self) -> watch::Receiver<bool> {
        self.lost.clone()
    }

    pub fn abort(&self) {
        self.handle.abort();
    }
}

/// Spawns a background task that refreshes shard leases every 10 seconds.
pub fn spawn_lease_heartbeat(
    cache: Arc<Cache>,
    range: ShardRange,
    node_id: String,
) -> LeaseHeartbeat {
    let (tx, rx) = watch::channel(false);
    let handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(HEARTBEAT_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut conn = cache.redis();
        let count = usize::from(range.end - range.start) + 1;
        let mut last_renewed = vec![Instant::now(); count];

        loop {
            interval.tick().await;
            let mut lost = false;

            for shard_id in range.start..=range.end {
                let index = usize::from(shard_id - range.start);
                let res = renew_one(&mut conn, shard_id, &node_id).await;

                match classify_refresh(&res) {
                    LeaseRefresh::Renewed => {
                        last_renewed[index] = Instant::now();
                        metrics::gauge!("lumi.shard.lease.owned", "shard" => shard_id.to_string())
                            .set(1.0);
                    }
                    outcome => {
                        if outcome == LeaseRefresh::Lost {
                            tracing::error!(
                                shard_id,
                                %node_id,
                                "shard lease lost or expired; fencing this shard"
                            );
                            metrics::counter!("lumi.shard.lease.lost").increment(1);
                        } else {
                            tracing::warn!(
                                shard_id,
                                %node_id,
                                error = %res.as_ref().err().map_or_else(String::new, ToString::to_string),
                                "shard lease refresh failed; retrying"
                            );
                            metrics::counter!("lumi.shard.lease.refresh_error").increment(1);
                        }
                        if should_fence(outcome, last_renewed[index], Instant::now(), LEASE_TTL) {
                            if outcome == LeaseRefresh::Transient {
                                tracing::error!(
                                    shard_id,
                                    %node_id,
                                    "shard lease could not be renewed before expiry; fencing this shard"
                                );
                            }
                            metrics::gauge!(
                                "lumi.shard.lease.owned",
                                "shard" => shard_id.to_string()
                            )
                            .set(0.0);
                            lost = true;
                        }
                    }
                }
            }

            if lost {
                tx.send_replace(true);
                break;
            }
        }
    });
    LeaseHeartbeat { lost: rx, handle }
}

#[cfg(test)]
mod tests {
    use super::{LeaseRefresh, RELEASE_SCRIPT, RENEW_SCRIPT, classify_refresh, should_fence};
    use std::time::{Duration, Instant};

    #[test]
    fn lease_scripts_check_ownership() {
        // Blind `SET XX` would let an expired owner steal a live lease back.
        assert!(RENEW_SCRIPT.contains("redis.call('get'"));
        assert!(!RENEW_SCRIPT.contains("'XX'"));
        assert!(RELEASE_SCRIPT.contains("redis.call('get'"));
        assert!(!RELEASE_SCRIPT.contains("'XX'"));
    }

    #[test]
    fn refresh_classification_distinguishes_lost_from_transient() {
        assert_eq!(
            classify_refresh(&Ok(Some("OK".into()))),
            LeaseRefresh::Renewed
        );
        assert_eq!(classify_refresh(&Ok(None)), LeaseRefresh::Lost);
        assert_eq!(
            classify_refresh(&Ok(Some("other-owner".into()))),
            LeaseRefresh::Lost
        );

        let io = redis::RedisError::from((redis::ErrorKind::IoError, "connection reset"));
        assert_eq!(
            classify_refresh(&Err(io)),
            LeaseRefresh::Transient,
            "transport failures must not fence immediately; expiry is the fence"
        );
    }

    #[test]
    fn fencing_follows_ownership_and_ttl() {
        let now = Instant::now();
        let fresh = now - Duration::from_secs(1);
        let stale = now - Duration::from_secs(31);
        let ttl = Duration::from_secs(30);

        assert!(!should_fence(LeaseRefresh::Renewed, stale, now, ttl));
        assert!(should_fence(LeaseRefresh::Lost, fresh, now, ttl));
        assert!(!should_fence(LeaseRefresh::Transient, fresh, now, ttl));
        assert!(should_fence(LeaseRefresh::Transient, stale, now, ttl));
    }
}
