use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
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
}

fn default_redis_url() -> String {
    "redis://127.0.0.1/".into()
}

fn default_rpc_port() -> u16 {
    8080
}

fn default_custom_dir() -> String {
    "addons".into()
}

impl Config {
    /// Load layered configuration:
    /// 1. Default fallback values
    /// 2. `lumi.toml` file (if present)
    /// 3. Environment variables (prefixed with `LUMI_`, or standard `DISCORD_TOKEN`, `DATABASE_URL`)
    pub fn load() -> Result<Self> {
        let mut figment = Figment::new()
            .merge(Toml::file("lumi.toml"))
            .merge(Env::prefixed("LUMI_"));

        if let Ok(tok) = std::env::var("DISCORD_TOKEN") {
            figment = figment.merge(("token", tok));
        }
        if let Ok(db) = std::env::var("DATABASE_URL") {
            figment = figment.merge(("database_url", db));
        }
        if let Ok(redis) = std::env::var("REDIS_URL") {
            figment = figment.merge(("redis_url", redis));
        }

        figment.extract().map_err(|err| LumiError::Config(err.to_string()))
    }

    pub fn from_env() -> Result<Self> {
        Self::load()
    }

    pub fn is_owner(&self, user_id: u64) -> bool {
        self.owner_ids.contains(&user_id)
    }
}
