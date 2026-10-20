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

/// Inclusive shard range this host owns out of the fleet total (`SHARDS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardRange {
    pub start: u16,
    pub end: u16,
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
    /// Inclusive slice of the fleet this host runs, from `SHARD_RANGE`.
    #[serde(default)]
    pub shard_range: Option<ShardRange>,
    /// Whether to request privileged `GUILD_MEMBERS` gateway intent. Defaults to false.
    #[serde(default)]
    pub intents_guild_members: bool,
    /// Discord application public key (Ed25519) for verifying HTTP interactions.
    pub discord_public_key: Option<String>,
    /// Whether dynamic Redis shard leasing coordinator is enabled.
    #[serde(default)]
    pub shard_coordinator: bool,
    /// Maximum number of shards allocated per node when coordinator is enabled.
    #[serde(default = "default_max_shards_per_node")]
    pub max_shards_per_node: u16,
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

fn default_max_shards_per_node() -> u16 {
    16
}

impl Config {
    /// Load process configuration directly from environment variables.
    pub fn load() -> Result<Self> {
        let token = std::env::var("BOT_TOKEN")
            .or_else(|_| std::env::var("DISCORD_TOKEN"))
            .map_err(|_| {
                LumiError::Config(
                    "BOT_TOKEN or DISCORD_TOKEN must be set in environment / .env".into(),
                )
            })?;

        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@127.0.0.1:5432/lumi".into());

        let redis_url =
            std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379/".into());

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

        let shard_range = match std::env::var("SHARD_RANGE") {
            Ok(raw) => Some(parse_shard_range(raw.trim(), shard_count)?),
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
                    let id = id_str.parse::<u64>().map_err(|_| {
                        LumiError::Config(format!("owner entry '{id_str}' is not a snowflake"))
                    })?;
                    if !owner_ids.contains(&id) {
                        owner_ids.push(id);
                    }
                }
            }
        }

        let intents_guild_members = std::env::var("INTENTS_GUILD_MEMBERS")
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        let discord_public_key = std::env::var("DISCORD_PUBLIC_KEY").ok();

        let shard_coordinator = std::env::var("SHARD_COORDINATOR")
            .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
            .unwrap_or(false);

        let max_shards_per_node = std::env::var("MAX_SHARDS_PER_NODE")
            .ok()
            .and_then(|v| v.trim().parse::<u16>().ok())
            .unwrap_or_else(default_max_shards_per_node);

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
            shard_range,
            intents_guild_members,
            discord_public_key,
            shard_coordinator,
            max_shards_per_node,
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

fn parse_shard_range(raw: &str, shard_count: Option<u16>) -> Result<ShardRange> {
    let (start, end) = raw.split_once('-').ok_or_else(|| {
        LumiError::Config(format!("SHARD_RANGE must be 'start-end', got '{raw}'"))
    })?;
    let start = start.trim().parse::<u16>().map_err(|_| {
        LumiError::Config(format!(
            "SHARD_RANGE start must be a shard index, got '{raw}'"
        ))
    })?;
    let end = end.trim().parse::<u16>().map_err(|_| {
        LumiError::Config(format!(
            "SHARD_RANGE end must be a shard index, got '{raw}'"
        ))
    })?;

    if start > end {
        return Err(LumiError::Config(format!(
            "SHARD_RANGE start {start} must not exceed end {end}"
        )));
    }
    let total = shard_count.ok_or_else(|| {
        LumiError::Config("SHARD_RANGE requires SHARDS (fleet total) to be set".into())
    })?;
    if end >= total {
        return Err(LumiError::Config(format!(
            "SHARD_RANGE end {end} must be below the fleet total {total}"
        )));
    }

    Ok(ShardRange { start, end })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shard_range_valid() {
        assert_eq!(
            parse_shard_range("3-7", Some(8)).ok(),
            Some(ShardRange { start: 3, end: 7 })
        );
        assert_eq!(
            parse_shard_range(" 3 - 7 ", Some(8)).ok(),
            Some(ShardRange { start: 3, end: 7 })
        );
        assert_eq!(
            parse_shard_range("0-0", Some(1)).ok(),
            Some(ShardRange { start: 0, end: 0 })
        );
    }

    #[test]
    fn shard_range_requires_fleet_total() {
        let error = parse_shard_range("0-3", None).err();
        assert!(matches!(error, Some(LumiError::Config(_))));
    }

    #[test]
    fn shard_range_rejects_reversed_bounds() {
        let error = parse_shard_range("5-4", Some(8)).err();
        assert!(matches!(error, Some(LumiError::Config(_))));
    }

    #[test]
    fn shard_range_rejects_out_of_bounds_end() {
        assert!(parse_shard_range("4-8", Some(8)).is_err());
        assert!(parse_shard_range("0-7", Some(8)).is_ok());
    }

    #[test]
    fn shard_range_rejects_malformed_input() {
        for raw in ["", "5", "a-b", "0-", "-3", "1-2-3", "1.5-2", "-1-2"] {
            assert!(
                matches!(
                    parse_shard_range(raw, Some(8)).err(),
                    Some(LumiError::Config(_))
                ),
                "expected error for '{raw}'"
            );
        }
    }
}
