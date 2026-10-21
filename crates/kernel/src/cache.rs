//! Two-tier cache: an in-process moka cache in front of Redis, with pub/sub
//! invalidation so every process drops matching local entries.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tokio::sync::broadcast;

use futures::StreamExt;
use moka::future::Cache as MokaCache;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use redis::{AsyncCommands, ExistenceCheck, SetExpiry, SetOptions};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Result;

pub const INVALIDATE_CHANNEL: &str = "lumi:invalidate";

const LOCAL_CAPACITY: u64 = 10_000;

/// Reconnect backoff bounds for the invalidation listener.
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(60);

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

fn info_field<T: std::str::FromStr>(info: &str, name: &str) -> Option<T> {
    info.lines()
        .find_map(|line| Some(line.strip_prefix(name)?.strip_prefix(':')?.trim()))
        .and_then(|value| value.parse().ok())
}

/// Payload stored for keys written through [`Cache::set_json_ttl`].
#[derive(Serialize, Deserialize)]
struct TtlEntry {
    exp: i64,
    v: Value,
}

impl TtlEntry {
    fn live(&self, now: i64) -> bool {
        self.exp > now
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as i64)
}

/// Request coalescer: concurrent callers of a missing key share one fetcher.
#[derive(Debug, Default)]
pub struct SingleFlight {
    in_flight: Mutex<HashMap<String, broadcast::Sender<std::result::Result<Value, String>>>>,
}

impl SingleFlight {
    pub async fn get_or_fetch<F, Fut>(&self, key: &str, fetcher: F) -> Result<Value>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Value>>,
    {
        let (rx, is_leader) = {
            let mut map = self
                .in_flight
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(tx) = map.get(key) {
                (tx.subscribe(), false)
            } else {
                let (tx, rx) = broadcast::channel(1);
                map.insert(key.to_string(), tx);
                (rx, true)
            }
        };

        if is_leader {
            struct Guard<'a> {
                key: &'a str,
                sf: &'a SingleFlight,
                active: bool,
            }
            impl<'a> Drop for Guard<'a> {
                fn drop(&mut self) {
                    if self.active {
                        let mut map = self
                            .sf
                            .in_flight
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        map.remove(self.key);
                    }
                }
            }
            let mut guard = Guard {
                key,
                sf: self,
                active: true,
            };

            let res = fetcher().await;

            guard.active = false;
            let mut map = self
                .in_flight
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(tx) = map.remove(key) {
                let to_send = match &res {
                    Ok(v) => Ok(v.clone()),
                    Err(e) => Err(e.to_string()),
                };
                let _ = tx.send(to_send);
            }
            res
        } else {
            let mut rx = rx;
            match rx.recv().await {
                Ok(Ok(val)) => Ok(val),
                Ok(Err(err)) => Err(crate::LumiError::Cache(err)),
                Err(_) => Err(crate::LumiError::Cache(
                    "singleflight sender dropped".into(),
                )),
            }
        }
    }
}

fn ttl_entry(value: &Value, ttl: Duration, now: i64) -> TtlEntry {
    TtlEntry {
        exp: now.saturating_add(ttl.as_millis() as i64),
        v: value.clone(),
    }
}

/// In-process cooldown claims, independent of Redis.
#[derive(Default)]
struct Claims {
    until: Mutex<HashMap<String, Instant>>,
}

impl Claims {
    fn claim(&self, key: &str, ttl: Duration) -> bool {
        let now = Instant::now();
        let mut until = self
            .until
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if until.get(key).is_some_and(|deadline| *deadline > now) {
            return false;
        }
        if until.len() >= LOCAL_CAPACITY as usize {
            until.retain(|_, deadline| *deadline > now);
            // Cooldowns are best-effort: dropping a live slot only costs an
            // extra database check, so this map never grows past its bound.
            while until.len() >= LOCAL_CAPACITY as usize {
                let Some(expired_key) = until.keys().next().cloned() else {
                    break;
                };
                until.remove(&expired_key);
            }
        }
        until.insert(key.to_string(), now + ttl);
        true
    }

    fn release(&self, key: &str) {
        let mut until = self
            .until
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        until.remove(key);
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum Invalidation {
    Keys(Vec<String>),
    Prefix(String),
}

pub struct Cache {
    local: MokaCache<String, Arc<TtlEntry>>,
    claims: Claims,
    client: redis::Client,
    redis: ConnectionManager,
    singleflight: SingleFlight,
}

impl std::fmt::Debug for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cache")
            .field("local", &self.local.entry_count())
            .field(
                "claims",
                &self.claims.until.lock().map_or(0, |map| map.len()),
            )
            .field("client", &self.client)
            .field("redis", &"ConnectionManager")
            .finish()
    }
}

impl Cache {
    pub async fn connect(redis_url: &str) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let config = ConnectionManagerConfig::new()
            .set_connection_timeout(CONNECT_TIMEOUT)
            .set_response_timeout(CONNECT_TIMEOUT);
        let redis = client.get_connection_manager_with_config(config).await?;
        Ok(Self {
            local: MokaCache::builder().max_capacity(LOCAL_CAPACITY).build(),
            claims: Claims::default(),
            client,
            redis,
            singleflight: SingleFlight::default(),
        })
    }

    pub fn redis(&self) -> ConnectionManager {
        self.redis.clone()
    }

    /// Atomically claim a cooldown slot. Returns `true` when the caller may
    /// proceed, `false` while the key is still held.
    pub async fn claim_cooldown(&self, key: &str, ttl_ms: u64) -> Result<bool> {
        let mut conn = self.redis.clone();
        let opts = SetOptions::default()
            .conditional_set(ExistenceCheck::NX)
            .with_expiration(SetExpiry::PX(ttl_ms));
        let set: Option<String> = conn.set_options(key, "1", opts).await?;
        Ok(set.is_some())
    }

    /// Remaining time on a cooldown key in milliseconds (`0` when unset).
    pub async fn cooldown_remaining_ms(&self, key: &str) -> Result<i64> {
        let mut conn = self.redis.clone();
        let ttl: i64 = conn.pttl(key).await?;
        Ok(ttl.max(0))
    }

    /// Round-trip time of a Redis command, in milliseconds. Uses `PING` so the
    /// health dashboard's polling does not inflate `keyspace_misses` (a `GET`
    /// on a missing key counts as a miss and would drag the hit ratio to 0%).
    pub async fn probe_read_ms(&self) -> Result<f64> {
        let mut conn = self.redis.clone();
        let started = std::time::Instant::now();
        let _: String = redis::cmd("PING").query_async(&mut conn).await?;
        Ok(started.elapsed().as_secs_f64() * 1000.0)
    }

    /// Share of Redis key lookups that hit, in percent. `None` until the server
    /// has served at least one lookup, so a fresh instance is not reported as
    /// "0%" (which reads as a broken cache rather than no traffic).
    pub async fn hit_ratio(&self) -> Result<Option<f64>> {
        let mut conn = self.redis.clone();
        let info: String = redis::cmd("INFO").query_async(&mut conn).await?;
        let hits = info_field::<u64>(&info, "keyspace_hits").unwrap_or(0);
        let misses = info_field::<u64>(&info, "keyspace_misses").unwrap_or(0);
        let lookups = hits + misses;
        Ok((lookups > 0).then(|| hits as f64 / lookups as f64 * 100.0))
    }

    pub async fn server_version(&self) -> Result<String> {
        let mut conn = self.redis.clone();
        let info: String = redis::cmd("INFO")
            .arg("server")
            .query_async(&mut conn)
            .await?;
        Ok(info_field::<String>(&info, "redis_version").unwrap_or_else(|| "unknown".to_string()))
    }

    /// Read a key whose payload carries its own absolute expiry. A local entry
    /// past its deadline is dropped and reported as a miss.
    pub async fn get_json_ttl(&self, key: &str) -> Result<Option<Value>> {
        let now = now_ms();
        if let Some(entry) = self.local.get(key).await {
            if entry.live(now) {
                return Ok(Some(entry.v.clone()));
            }
            self.local.invalidate(key).await;
            return Ok(None);
        }
        let mut conn = self.redis.clone();
        let raw: Option<String> = conn.get(key).await?;
        let Some(raw) = raw else {
            return Ok(None);
        };
        let entry: TtlEntry = serde_json::from_str(&raw)?;
        if !entry.live(now) {
            return Ok(None);
        }
        // Stored as a struct: a local hit skips parsing the payload again.
        let value = entry.v.clone();
        self.local.insert(key.to_string(), Arc::new(entry)).await;
        Ok(Some(value))
    }

    /// Write a key with a Redis expiry plus an in-payload deadline, so a local
    /// hit never outlives the Redis entry.
    pub async fn set_json_ttl(&self, key: &str, value: &Value, ttl: Duration) -> Result<()> {
        let entry = ttl_entry(value, ttl, now_ms());
        let raw = serde_json::to_string(&entry)?;
        let ttl_ms = ttl.as_millis().min(u128::from(u64::MAX)) as u64;
        let mut conn = self.redis.clone();
        let opts = SetOptions::default().with_expiration(SetExpiry::PX(ttl_ms));
        let _: Option<String> = conn.set_options(key, raw, opts).await?;
        // Persist first: a failed Redis write must not leave this process
        // serving a value no other process can see.
        self.local.insert(key.to_string(), Arc::new(entry)).await;
        Ok(())
    }

    /// Get a cached value or fetch it exactly once across concurrent callers.
    pub async fn get_or_fetch_json<F, Fut>(
        &self,
        key: &str,
        ttl: Duration,
        fetcher: F,
    ) -> Result<Value>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<Value>>,
    {
        self.get_or_fetch_json_with_ttl(key, || async move {
            let value = fetcher().await?;
            Ok((value, ttl))
        })
        .await
    }

    /// Like [`Cache::get_or_fetch_json`], but the fetcher picks the TTL so a
    /// negative result can expire sooner than a positive one.
    pub async fn get_or_fetch_json_with_ttl<F, Fut>(&self, key: &str, fetcher: F) -> Result<Value>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<(Value, Duration)>>,
    {
        if let Some(v) = self.get_json_ttl(key).await? {
            return Ok(v);
        }

        self.singleflight
            .get_or_fetch(key, || async {
                let (value, ttl) = fetcher().await?;
                self.set_json_ttl(key, &value, ttl).await?;
                Ok(value)
            })
            .await
    }

    /// Claim a short in-process cooldown slot. No Redis round trip; the slot is
    /// only shared inside this process.
    pub fn claim_local(&self, key: &str, ttl: Duration) -> bool {
        self.claims.claim(key, ttl)
    }

    /// Drop an in-process cooldown claim whose guarded work did not happen, so
    /// the next call can retry instead of waiting out the TTL.
    pub fn release_local(&self, key: &str) {
        self.claims.release(key);
    }

    pub async fn invalidate(&self, key: &str) -> Result<()> {
        self.local.invalidate(key).await;
        let mut conn = self.redis.clone();
        let _: i64 = conn.del(key).await?;
        self.publish(&Invalidation::Keys(vec![key.to_string()]))
            .await
    }

    pub async fn invalidate_prefix(&self, prefix: &str) -> Result<()> {
        let local_keys: Vec<String> = self
            .local
            .iter()
            .map(|(key, _)| key.to_string())
            .filter(|key| key.starts_with(prefix))
            .collect();
        for key in local_keys {
            self.local.invalidate(&key).await;
        }

        let mut conn = self.redis.clone();
        let mut cursor = 0_u64;
        loop {
            let (next, batch): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(format!("{prefix}*"))
                .arg("COUNT")
                .arg(100)
                .query_async(&mut conn)
                .await?;
            if !batch.is_empty() {
                let _: i64 = conn.del(batch.as_slice()).await?;
            }
            cursor = next;
            if cursor == 0 {
                break;
            }
        }
        self.publish(&Invalidation::Prefix(prefix.to_string()))
            .await
    }

    async fn publish(&self, msg: &Invalidation) -> Result<()> {
        let payload = serde_json::to_string(msg)?;
        let mut conn = self.redis.clone();
        let _: i64 = conn.publish(INVALIDATE_CHANNEL, payload).await?;
        Ok(())
    }

    /// Subscribe to [`INVALIDATE_CHANNEL`] and drop matching local keys.
    ///
    /// Never returns; call from a spawned task. Reconnects with exponential
    /// backoff capped at [`BACKOFF_MAX`].
    pub async fn spawn_invalidation(self: Arc<Self>) {
        let mut backoff = BACKOFF_MIN;
        loop {
            match self.subscribe_loop().await {
                Ok(()) => backoff = BACKOFF_MIN,
                Err(err) => {
                    tracing::warn!(error = %err, backoff_secs = backoff.as_secs(), "cache invalidation listener dropped; resubscribing");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(BACKOFF_MAX);
                }
            }
        }
    }

    async fn subscribe_loop(&self) -> Result<()> {
        let mut pubsub = self.client.get_async_pubsub().await?;
        pubsub.subscribe(INVALIDATE_CHANNEL).await?;
        let mut stream = pubsub.on_message();
        while let Some(msg) = stream.next().await {
            let payload: String = msg.get_payload()?;
            match serde_json::from_str::<Invalidation>(&payload) {
                Ok(Invalidation::Keys(keys)) => {
                    for key in keys {
                        self.local.invalidate(&key).await;
                    }
                }
                Ok(Invalidation::Prefix(prefix)) => {
                    let keys: Vec<String> = self
                        .local
                        .iter()
                        .map(|(key, _)| key.to_string())
                        .filter(|key| key.starts_with(&prefix))
                        .collect();
                    for key in keys {
                        self.local.invalidate(&key).await;
                    }
                }
                Err(err) => tracing::warn!(error = %err, "bad invalidation payload"),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Claims, LOCAL_CAPACITY, SingleFlight, TtlEntry, now_ms, ttl_entry};
    use serde_json::{Value, json};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[test]
    fn ttl_entry_expires() {
        let now = now_ms();
        let stored = ttl_entry(&json!("value"), Duration::from_secs(60), now);
        assert!(stored.live(now));
        assert_eq!(stored.v, json!("value"));
        assert!(!stored.live(now + 60_000));
        assert!(!stored.live(now + 60_001));
    }

    #[test]
    fn malformed_entry_is_a_miss() {
        assert!(serde_json::from_str::<TtlEntry>(r#"{"exp": 1}"#).is_err());
    }

    #[test]
    fn claims_map_stays_bounded_when_every_slot_is_live() {
        let claims = Claims::default();
        for index in 0..LOCAL_CAPACITY {
            assert!(claims.claim(&format!("k{index}"), Duration::from_secs(60)));
        }
        assert!(claims.claim("one-more", Duration::from_secs(60)));
        let len = claims
            .until
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len();
        assert_eq!(len, LOCAL_CAPACITY as usize);
    }

    #[test]
    fn claim_is_held_until_expiry() {
        let claims = Claims::default();
        assert!(claims.claim("k", Duration::from_millis(50)));
        assert!(!claims.claim("k", Duration::from_millis(50)));
        std::thread::sleep(Duration::from_millis(60));
        assert!(claims.claim("k", Duration::from_millis(50)));
    }

    #[test]
    fn released_claim_can_be_reclaimed() {
        let claims = Claims::default();
        assert!(claims.claim("k", Duration::from_secs(60)));
        claims.release("k");
        assert!(claims.claim("k", Duration::from_secs(60)));
    }

    #[test]
    fn expired_claim_is_replaceable() {
        let claims = Claims::default();
        assert!(claims.claim("k", Duration::ZERO));
        assert!(claims.claim("k", Duration::from_secs(1)));
        assert!(!claims.claim("k", Duration::from_secs(1)));
    }

    #[tokio::test]
    async fn singleflight_coalesces_concurrent_fetches() {
        let sf = Arc::new(SingleFlight::default());
        let call_count = Arc::new(Mutex::new(0));
        let barrier = Arc::new(tokio::sync::Barrier::new(10));

        let handles: Vec<_> = (0..10)
            .map(|_| {
                let sf = sf.clone();
                let call_count = call_count.clone();
                let barrier = barrier.clone();
                tokio::spawn(async move {
                    barrier.wait().await;
                    sf.get_or_fetch("key", || async {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        let mut n = call_count
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        *n += 1;
                        Ok::<Value, crate::LumiError>(json!({"x": *n}))
                    })
                    .await
                })
            })
            .collect();

        for h in handles {
            let res = h.await;
            assert!(matches!(res, Ok(Ok(_))));
        }

        let final_count = *call_count
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(final_count, 1, "fetcher should run exactly once");
    }

    #[tokio::test]
    async fn singleflight_returns_error_to_all_waiters() {
        let sf = Arc::new(SingleFlight::default());
        let attempt = Arc::new(Mutex::new(0));
        let barrier = Arc::new(tokio::sync::Barrier::new(5));

        let handles: Vec<_> = (0..5)
            .map(|_| {
                let sf = sf.clone();
                let attempt = attempt.clone();
                let barrier = barrier.clone();
                tokio::spawn(async move {
                    barrier.wait().await;
                    sf.get_or_fetch("key", || async {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        let mut n = attempt
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                        *n += 1;
                        Err::<Value, crate::LumiError>(crate::LumiError::Cache("fail".into()))
                    })
                    .await
                })
            })
            .collect();

        for h in handles {
            let res = h.await;
            assert!(matches!(res, Ok(Err(_))));
        }

        let final_attempts = *attempt
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(
            final_attempts, 1,
            "fetcher should run exactly once even on error"
        );
    }
}
