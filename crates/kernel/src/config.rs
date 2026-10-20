//! Process configuration: direct environment-based loader (`.env`)
//! and the deployment [`Role`] the binary runs as.

use serde::{Deserialize, Serialize};

use crate::{LumiError, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    #[default]
    All,
    Gateway,
    Worker,
    Api,
    Scheduler,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub role: Role,
    pub token: String,
    pub database_url: String,
    #[serde(default = "default_redis_url")]
    pub redis_url: String,
    pub rpc_token: Option<String>,
    #[serde(default = "default_rpc_port")]
    pub rpc_port: u16,
    #[serde(default)]
    pub owner_ids: Vec<u64>,
    #[serde(default = "default_custom_dir")]
    pub custom_dir: String,
    /// Shards this process should run, overriding Discord's recommendation.
    #[serde(default)]
    pub shard_count: Option<u16>,
}

fn default_redis_url() -> String {
    "redis://127.0.0.1:6379/".into()
}

fn default_rpc_port() -> u16 {
    8080
}

fn default_custom_dir() -> String {
    "addons".into()
}

impl Config {
    /// Load process configuration directly from environment variables.
    pub fn load() -> Result<Self> {
        let token = std::env::var("BOT_TOKEN")
            .or_else(|_| std::env::var("DISCORD_TOKEN"))
            .map_err(|_| LumiError::Config("BOT_TOKEN or DISCORD_TOKEN must be set in environment / .env".into()))?;

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/lumi".into());

        let redis_url = std::env::var("REDIS_URL")
            .unwrap_or_else(|_| "redis://127.0.0.1:6379/".into());

        let rpc_token = std::env::var("RPC_TOKEN").ok();
        let rpc_port = std::env::var("RPC_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or_else(default_rpc_port);

        let custom_dir = std::env::var("CUSTOM_DIR").unwrap_or_else(|_| default_custom_dir());

        let shard_count = match std::env::var("SHARDS") {
            Ok(raw) => Some(
                raw.trim()
                    .parse()
                    .ok()
                    .filter(|count| *count > 0)
                    .ok_or_else(|| {
                        LumiError::Config(format!("SHARDS must be a positive count, got '{raw}'"))
                    })?,
            ),
            Err(_) => None,
        };

        let mut owner_ids = Vec::new();
        if let Ok(owners) = std::env::var("OWNER")
            .or_else(|_| std::env::var("OWNERS"))
            .or_else(|_| std::env::var("OWNER_IDS"))
        {
            for id_str in owners.split(',') {
                let id_str = id_str.trim();
                if !id_str.is_empty() {
                    let id = id_str
                        .parse::<u64>()
                        .map_err(|_| LumiError::Config(format!("owner entry '{id_str}' is not a snowflake")))?;
                    if !owner_ids.contains(&id) {
                        owner_ids.push(id);
                    }
                }
            }
        }

        let cfg = Self {
            role: Role::All,
            token,
            database_url,
            redis_url,
            rpc_token,
            rpc_port,
            owner_ids,
            custom_dir,
            shard_count,
        };
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        if self.database_url.trim().is_empty() {
            return Err(LumiError::Config("database_url must not be empty".into()));
        }
        if self.redis_url.trim().is_empty() {
            return Err(LumiError::Config("redis_url must not be empty".into()));
        }
        Ok(())
    }

    #[must_use]
    pub fn is_owner(&self, user_id: u64) -> bool {
        self.owner_ids.contains(&user_id)
    }
}
