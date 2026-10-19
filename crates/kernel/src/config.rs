//! Process configuration: one layered loader (file → `LUMI_*` env → common
//! deployment env vars) and the deployment [`Role`] the binary runs as.

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
    /// 1. `lumi.toml` (optional)
    /// 2. `LUMI_*` environment variables
    /// 3. common deployment names (`DISCORD_TOKEN`, `DATABASE_URL`, `REDIS_URL`, `OWNER_IDS`)
    pub fn load() -> Result<Self> {
        let mut figment = Figment::new()
            .merge(Toml::file("lumi.toml"))
            .merge(Env::prefixed("LUMI_"));

        for (env, key) in [
            ("DISCORD_TOKEN", "token"),
            ("DATABASE_URL", "database_url"),
            ("REDIS_URL", "redis_url"),
        ] {
            if let Ok(value) = std::env::var(env) {
                figment = figment.merge((key, value));
            }
        }

        // `owner_ids` is a list, which figment cannot parse out of a single env
        // string; parse it here and let it override any raw provider value.
        if let Ok(owners) = std::env::var("OWNER_IDS").or_else(|_| std::env::var("LUMI_OWNER_IDS")) {
            let ids = owners
                .split(',')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(|id| {
                    id.parse::<u64>()
                        .map_err(|_| LumiError::Config(format!("OWNER_IDS entry '{id}' is not a snowflake")))
                })
                .collect::<Result<Vec<_>>>()?;
            figment = figment.merge(("owner_ids", ids));
        }

        let cfg: Self = figment.extract().map_err(|err| LumiError::Config(err.to_string()))?;
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
