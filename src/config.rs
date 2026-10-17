use crate::{LumiError, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    All,
    Gateway,
    Worker,
    Api,
    Scheduler,
}

impl Role {
    pub fn from_env() -> Self {
        match std::env::var("LUMI_ROLE").ok().as_deref() {
            Some("gateway") => Self::Gateway,
            Some("worker") => Self::Worker,
            Some("api") => Self::Api,
            Some("scheduler") => Self::Scheduler,
            _ => Self::All,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Config {
    pub role: Role,
    pub token: String,
    pub database_url: String,
    pub redis_url: String,
    pub rpc_token: Option<String>,
    pub rpc_port: u16,
    pub owner_ids: Vec<u64>,
    pub custom_dir: String,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        let role = Role::from_env();
        let token = required("DISCORD_TOKEN")?;
        let database_url = required("DATABASE_URL")?;
        let redis_url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1/".into());
        let rpc_token = std::env::var("RPC_TOKEN").ok();
        let rpc_port = std::env::var("RPC_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080);
        let owner_ids = std::env::var("OWNER_IDS")
            .unwrap_or_default()
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();
        let custom_dir = std::env::var("CUSTOM_DIR").unwrap_or_else(|_| "addons".into());

        Ok(Config {
            role,
            token,
            database_url,
            redis_url,
            rpc_token,
            rpc_port,
            owner_ids,
            custom_dir,
        })
    }

    pub fn is_owner(&self, user_id: u64) -> bool {
        self.owner_ids.contains(&user_id)
    }
}

fn required(key: &str) -> Result<String> {
    std::env::var(key).map_err(|_| LumiError::Config(format!("missing required env var {key}")))
}
