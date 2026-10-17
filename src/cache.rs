//! Two-tier cache: an in-process moka cache in front of Redis, with pub/sub
//! invalidation so every process drops matching local entries.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use moka::future::Cache as MokaCache;
use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::Result;

/// Channel carrying cross-process invalidation messages.
pub const INVALIDATE_CHANNEL: &str = "lumi:invalidate";

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
        let redis = client
            .get_connection_manager_with_config(ConnectionManagerConfig::new())
            .await?;
        Ok(Self {
            local: MokaCache::builder().max_capacity(10_000).build(),
            client,
            redis,
        })
    }

    pub async fn get_json(&self, key: &str) -> Result<Option<Value>> {
        if let Some(value) = self.local.get(key).await {
            return Ok(Some(value));
        }
        let mut conn = self.redis.clone();
        let raw: Option<String> = redis::cmd("GET").arg(key).query_async(&mut conn).await?;
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
        let _: String = redis::cmd("SET").arg(key).arg(raw).query_async(&mut conn).await?;
        Ok(())
    }

    /// Return the cached value, otherwise compute it, store and return it.
    pub async fn get_or_insert_json<F, Fut>(&self, key: &str, compute: F) -> Result<Value>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Value>>,
    {
        if let Some(value) = self.get_json(key).await? {
            return Ok(value);
        }
        let value = compute().await?;
        self.set_json(key, &value).await?;
        Ok(value)
    }

    pub async fn invalidate(&self, key: &str) -> Result<()> {
        self.local.invalidate(key).await;
        let mut conn = self.redis.clone();
        let _: i64 = redis::cmd("DEL").arg(key).query_async(&mut conn).await?;
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
                let _: i64 = redis::cmd("DEL").arg(&batch).query_async(&mut conn).await?;
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
        let _: i64 = redis::cmd("PUBLISH")
            .arg(INVALIDATE_CHANNEL)
            .arg(payload)
            .query_async(&mut conn)
            .await?;
        Ok(())
    }

    /// Subscribe to [`INVALIDATE_CHANNEL`] and drop matching local keys.
    ///
    /// Never returns; call from a spawned task.
    pub async fn spawn_invalidation(self: Arc<Self>) {
        // shortcut: one dedicated pub/sub connection, no reconnect backoff cap;
        // back off harder or share a connection pool past a handful of workers.
        loop {
            if let Err(err) = self.subscribe_loop().await {
                tracing::warn!(error = %err, "cache invalidation listener dropped; resubscribing");
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
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
