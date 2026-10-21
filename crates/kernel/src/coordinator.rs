//! Dynamic Redis shard leasing coordinator for container / Kubernetes fleets.
//!
//! A lease is only useful if the owner stops serving when it can no longer
//! prove ownership. The heartbeat returns a watch channel that fires `true`
//! once the lease is lost or cannot be refreshed within its TTL, so the
//! gateway can shut its shards down instead of running a split-brain session.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::watch;

use crate::cache::Cache;
use crate::config::ShardRange;
use crate::error::Result;

/// Lease lifetime in Redis. A node must refresh well before this elapses.
const LEASE_TTL: Duration = Duration::from_secs(30);
/// Refresh cadence: three chances per TTL.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

fn lease_key(shard_id: u16) -> String {
    format!("lumi:shard_lease:{shard_id}")
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
            let res: Option<String> = redis::cmd("SET")
                .arg(lease_key(shard_id))
                .arg(node_id)
                .arg("NX")
                .arg("EX")
                .arg(LEASE_TTL.as_secs())
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

        // Partial claim rollback to avoid dangling fragments.
        for shard_id in start..(start + claimed) {
            let rollback: redis::RedisResult<()> = redis::cmd("DEL")
                .arg(lease_key(shard_id))
                .query_async(&mut conn)
                .await;
            if let Err(err) = rollback {
                tracing::warn!(
                    shard_id,
                    %node_id,
                    error = %err,
                    "failed to roll back a partially claimed shard lease"
                );
            }
        }

        // Skip the shard that broke the run so the next block starts after it.
        start += claimed + 1;
    }

    Ok(None)
}

/// One refresh pass across every shard this node owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LeaseRound {
    Refreshed,
    /// Redis answered and the key is gone (expired or claimed elsewhere).
    Lost,
    /// Redis did not answer; ownership is unknown until the TTL lapses.
    Unreachable,
}

/// Fencing state for one node's lease.
///
/// A missing key is proof of loss. A Redis outage is not, but ownership cannot
/// be proven past the TTL, so work must stop by then.
#[derive(Debug)]
struct LeaseHealth {
    ttl: Duration,
    last_confirmed: Instant,
}

impl LeaseHealth {
    fn new(now: Instant, ttl: Duration) -> Self {
        Self {
            ttl,
            last_confirmed: now,
        }
    }

    /// Returns whether the node may keep serving after this round.
    fn observe(&mut self, round: LeaseRound, now: Instant) -> bool {
        match round {
            LeaseRound::Refreshed => {
                self.last_confirmed = now;
                true
            }
            LeaseRound::Lost => false,
            LeaseRound::Unreachable => {
                now.saturating_duration_since(self.last_confirmed) < self.ttl
            }
        }
    }
}

async fn refresh_round(
    conn: &mut redis::aio::ConnectionManager,
    range: ShardRange,
    node_id: &str,
) -> LeaseRound {
    let mut outcome = LeaseRound::Refreshed;

    for shard_id in range.start..=range.end {
        let result: redis::RedisResult<Option<String>> = redis::cmd("SET")
            .arg(lease_key(shard_id))
            .arg(node_id)
            .arg("XX")
            .arg("EX")
            .arg(LEASE_TTL.as_secs())
            .query_async(&mut *conn)
            .await;

        match result {
            Ok(Some(_)) => {}
            Ok(None) => {
                tracing::error!(shard_id, %node_id, "shard lease expired or was reassigned");
                return LeaseRound::Lost;
            }
            Err(err) => {
                tracing::error!(
                    shard_id,
                    %node_id,
                    error = %err,
                    "shard lease refresh command failed"
                );
                outcome = LeaseRound::Unreachable;
            }
        }
    }

    outcome
}

/// Spawn a background task that refreshes shard leases every 10 seconds.
///
/// The returned receiver becomes `true` exactly once, when the node must stop
/// running its shards. The task exits after firing it.
pub fn spawn_lease_heartbeat(
    cache: Arc<Cache>,
    range: ShardRange,
    node_id: String,
) -> watch::Receiver<bool> {
    let (lost_tx, lost_rx) = watch::channel(false);

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(HEARTBEAT_INTERVAL);
        let mut conn = cache.redis();
        let mut health = LeaseHealth::new(Instant::now(), LEASE_TTL);

        loop {
            interval.tick().await;
            let round = refresh_round(&mut conn, range, &node_id).await;
            let held = health.observe(round, Instant::now());

            if !held {
                tracing::error!(
                    start = range.start,
                    end = range.end,
                    %node_id,
                    "shard lease lost; stopping shards to avoid duplicate sessions"
                );
                lost_tx.send_replace(true);
                return;
            }

            if round == LeaseRound::Unreachable {
                tracing::warn!(
                    start = range.start,
                    end = range.end,
                    %node_id,
                    "cannot refresh shard leases; fencing in at most {}s",
                    LEASE_TTL.as_secs()
                );
            }
        }
    });

    lost_rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ttl() -> Duration {
        Duration::from_secs(30)
    }

    #[test]
    fn refreshed_round_keeps_the_lease() {
        let now = Instant::now();
        let mut health = LeaseHealth::new(now, ttl());
        assert!(health.observe(LeaseRound::Refreshed, now + Duration::from_secs(10)));
    }

    #[test]
    fn missing_key_fences_immediately() {
        let now = Instant::now();
        let mut health = LeaseHealth::new(now, ttl());
        assert!(health.observe(LeaseRound::Refreshed, now + Duration::from_secs(10)));
        assert!(!health.observe(LeaseRound::Lost, now + Duration::from_secs(11)));
    }

    #[test]
    fn redis_outage_fences_after_the_ttl() {
        let now = Instant::now();
        let mut health = LeaseHealth::new(now, ttl());
        assert!(health.observe(LeaseRound::Unreachable, now + Duration::from_secs(10)));
        assert!(health.observe(LeaseRound::Unreachable, now + Duration::from_secs(20)));
        assert!(!health.observe(LeaseRound::Unreachable, now + ttl()));
    }

    #[test]
    fn recovery_extends_the_deadline() {
        let now = Instant::now();
        let mut health = LeaseHealth::new(now, ttl());
        assert!(health.observe(LeaseRound::Unreachable, now + Duration::from_secs(20)));
        assert!(health.observe(LeaseRound::Refreshed, now + Duration::from_secs(25)));
        assert!(health.observe(LeaseRound::Unreachable, now + ttl()));
        assert!(!health.observe(LeaseRound::Unreachable, now + Duration::from_secs(55)));
    }
}
