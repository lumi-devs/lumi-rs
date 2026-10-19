//! Two-tier cache: an in-process moka cache in front of Redis, with pub/sub
//! invalidation so every process drops matching local entries.

use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use moka::future::Cache as MokaCache;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use redis::{AsyncCommands, ExistenceCheck, SetExpiry, SetOptions};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Result;

/// Channel carrying cross-process invalidation messages.
pub const INVALIDATE_CHANNEL: &str = "lumi:invalidate";

/// Maximum number of entries held in each process's local tier.
const LOCAL_CAPACITY: u64 = 10_000;

/// Reconnect backoff bounds for the invalidation listener.
const BACKOFF_MIN: Duration = Duration::from_secs(1);
const BACKOFF_MAX: Duration = Duration::from_secs(60);

/// How long a Redis connection may take to establish or answer.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Serialize, Deserialize)]
enum Invalidation {
    Keys(Vec<String>),
    Prefix(String),
}

/// Local moka cache backed by a shared Redis connection.
pub struct Cache {
    local: MokaCache<String, Value>,
    client: redis::Client,
    redis: ConnectionManager,
}

impl std::fmt::Debug for Cache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cache")
            .field("local", &self.local.entry_count())
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
            client,
            redis,
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

    pub async fn get_json(&self, key: &str) -> Result<Option<Value>> {
        if let Some(value) = self.local.get(key).await {
            return Ok(Some(value));
        }
        let mut conn = self.redis.clone();
        let raw: Option<String> = conn.get(key).await?;
        let Some(raw) = raw else {
            return Ok(None);
        };
        let value: Value = serde_json::from_str(&raw)?;
        self.local.insert(key.to_string(), value.clone()).await;
        Ok(Some(value))
    }

    pub async fn set_json(&self, key: &str, value: &Value) -> Result<()> {
        self.local.insert(key.to_string(), value.clone()).await;
        let raw = serde_json::to_string(value)?;
        let mut conn = self.redis.clone();
        let _: () = conn.set(key, raw).await?;
        Ok(())
    }

    /// Typed read: deserialize the cached JSON into `T` instead of handing
    /// callers a raw [`Value`].
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Result<Option<T>> {
        Ok(self
            .get_json(key)
            .await?
            .map(serde_json::from_value)
            .transpose()?)
    }

    /// Typed write: serialize `T` as the cached JSON payload.
    pub async fn set<T: Serialize>(&self, key: &str, value: &T) -> Result<()> {
        self.set_json(key, &serde_json::to_value(value)?).await
    }

    pub async fn invalidate(&self, key: &str) -> Result<()> {
        self.local.invalidate(key).await;
        let mut conn = self.redis.clone();
        let _: i64 = conn.del(key).await?;
        self.publish(&Invalidation::Keys(vec![key.to_string()])).await
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
        self.publish(&Invalidation::Prefix(prefix.to_string())).await
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
