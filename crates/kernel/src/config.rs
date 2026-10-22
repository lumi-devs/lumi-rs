//! Process configuration: environment-based loader (`.env`) and the deployment
//! [`Role`] the binary runs as.
//!
//! Loading and validation are separate: the CLI picks the role, then
//! [`Config::validate_for`] rejects combinations that would boot but never
//! work (missing gateway token, missing interaction key, unusable URLs).

use serde::{Deserialize, Serialize};

use crate::{LumiError, Result};

/// Deployment role determining process capabilities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Run all bot subsystems.
    #[default]
    All,
    /// Run Discord gateway session.
    Gateway,
    /// Run background task worker.
    Worker,
    /// Run HTTP interaction API.
    Api,
}

impl Role {
    /// Whether this role holds a Discord gateway session.
    pub const fn is_gateway(self) -> bool {
        matches!(self, Self::All | Self::Gateway)
    }
}

/// Inclusive shard range this host owns out of the fleet total (`SHARDS`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShardRange {
    /// Start shard index (inclusive).
    pub start: u16,
    /// End shard index (inclusive).
    pub end: u16,
}

impl ShardRange {
    /// Serenity takes an exclusive range, while our config and leases are
    /// inclusive, so the upper bound must be advanced.
    pub const fn start_range(self) -> std::ops::Range<u16> {
        self.start..self.end.saturating_add(1)
    }
}

/// Global kernel configuration loaded from environment.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    /// Deployment role.
    #[serde(default)]
    pub role: Role,
    /// Discord bot token.
    pub token: String,
    /// Primary database connection string.
    pub database_url: String,
    /// Read replica for lag-tolerant queries; falls back to `database_url`.
    #[serde(default)]
    pub database_read_url: Option<String>,
    /// Redis connection string.
    #[serde(default = "default_redis_url")]
    pub redis_url: String,
    /// Optional RPC authentication token.
    pub rpc_token: Option<String>,
    /// RPC listening port.
    #[serde(default = "default_rpc_port")]
    pub rpc_port: u16,
    /// List of bot owner user IDs.
    #[serde(default)]
    pub owner_ids: Vec<u64>,
    /// Path to custom addons directory.
    #[serde(default = "default_custom_dir")]
    pub custom_dir: String,
    /// Fleet-wide total shard count (`SHARDS`).
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
    /// Whether shard coordinator is enabled.
    #[serde(default)]
    pub shard_coordinator: bool,
    /// Maximum shards assigned per node.
    #[serde(default = "default_max_shards_per_node")]
    pub max_shards_per_node: u16,
}

fn default_redis_url() -> String {
    "redis://127.0.0.1:6379/".into()
}

const fn default_rpc_port() -> u16 {
    8080
}

fn default_custom_dir() -> String {
    "addons".into()
}

const fn default_max_shards_per_node() -> u16 {
    16
}

/// Read a non-empty, trimmed environment variable.
fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn env_count(name: &str) -> Result<Option<u16>> {
    env(name)
        .map(|raw| {
            raw.parse::<u16>()
                .ok()
                .filter(|count| *count > 0)
                .ok_or_else(|| {
                    LumiError::Config(format!("{name} must be a positive count, got '{raw}'"))
                })
        })
        .transpose()
}

impl Config {
    /// Read configuration from environment variables without enforcing role
    /// invariants, so the CLI can select the role first.
    pub fn load() -> Result<Self> {
        let token = env("BOT_TOKEN")
            .or_else(|| env("DISCORD_TOKEN"))
            .unwrap_or_default();

        let database_url = env("DATABASE_URL")
            .unwrap_or_else(|| "postgres://postgres:postgres@127.0.0.1:5432/lumi".into());

        let redis_url = env("REDIS_URL").unwrap_or_else(default_redis_url);

        let rpc_token = env("RPC_TOKEN");
        let rpc_port = match env("RPC_PORT") {
            Some(raw) => raw.parse().map_err(|_| {
                LumiError::Config(format!("RPC_PORT must be a port number, got '{raw}'"))
            })?,
            None => default_rpc_port(),
        };

        let custom_dir = env("CUSTOM_DIR").unwrap_or_else(default_custom_dir);

        let shard_count = env_count("SHARDS")?;

        let shard_range = match env("SHARD_RANGE") {
            Some(raw) => Some(parse_shard_range(&raw, shard_count)?),
            None => None,
        };

        let mut owner_ids = Vec::new();
        if let Some(owners) = env("OWNER")
            .or_else(|| env("OWNERS"))
            .or_else(|| env("OWNER_IDS"))
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

        let intents_guild_members = env("INTENTS_GUILD_MEMBERS")
            .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"));

        let max_shards_per_node = match env("MAX_SHARDS_PER_NODE") {
            Some(raw) => raw.parse::<u16>().ok().ok_or_else(|| {
                LumiError::Config(format!(
                    "MAX_SHARDS_PER_NODE must be a shard count, got '{raw}'"
                ))
            })?,
            None => default_max_shards_per_node(),
        };

        Ok(Self {
            role: Role::All,
            token,
            database_url,
            database_read_url: env("DATABASE_READ_URL"),
            redis_url,
            rpc_token,
            rpc_port,
            owner_ids,
            custom_dir,
            shard_count,
            shard_range,
            intents_guild_members,
            discord_public_key: env("DISCORD_PUBLIC_KEY"),
            shard_coordinator: env("SHARD_COORDINATOR")
                .is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes")),
            max_shards_per_node,
        })
    }

    /// Reject a configuration that cannot serve `role`.
    pub fn validate_for(&self, role: Role) -> Result<()> {
        if !has_scheme(&self.database_url, &["postgres://", "postgresql://"]) {
            return Err(LumiError::Config(format!(
                "DATABASE_URL must be a postgres:// URL, got '{}'",
                self.database_url
            )));
        }

        if let Some(url) = &self.database_read_url
            && !has_scheme(url, &["postgres://", "postgresql://"])
        {
            return Err(LumiError::Config(format!(
                "DATABASE_READ_URL must be a postgres:// URL, got '{url}'"
            )));
        }

        if !has_scheme(&self.redis_url, &["redis://", "rediss://"]) {
            return Err(LumiError::Config(format!(
                "REDIS_URL must be a redis:// URL, got '{}'",
                self.redis_url
            )));
        }

        if role.is_gateway() && self.token.trim().is_empty() {
            return Err(LumiError::Config(
                "gateway roles require BOT_TOKEN or DISCORD_TOKEN".into(),
            ));
        }

        match &self.discord_public_key {
            Some(key) if !is_ed25519_hex(key) => {
                return Err(LumiError::Config(
                    "DISCORD_PUBLIC_KEY must be 64 hex characters (a 32-byte Ed25519 key)".into(),
                ));
            }
            None if matches!(role, Role::Api | Role::All) => {
                return Err(LumiError::Config(
                    "DISCORD_PUBLIC_KEY is required for the api and all roles; without it every interaction is rejected with 401".into(),
                ));
            }
            _ => {}
        }

        if self.shard_count == Some(0) {
            return Err(LumiError::Config(
                "SHARDS must be greater than zero when set".into(),
            ));
        }

        if self.shard_coordinator {
            if self.shard_count.is_none() {
                return Err(LumiError::Config(
                    "SHARD_COORDINATOR requires SHARDS (the fleet total)".into(),
                ));
            }
            if self.max_shards_per_node == 0 {
                return Err(LumiError::Config(
                    "MAX_SHARDS_PER_NODE must be greater than zero".into(),
                ));
            }
        }

        if let Some(range) = self.shard_range {
            let Some(total) = self.shard_count else {
                return Err(LumiError::Config(
                    "SHARD_RANGE requires SHARDS (the fleet total)".into(),
                ));
            };
            if range.start > range.end {
                return Err(LumiError::Config(format!(
                    "SHARD_RANGE start {} must not exceed end {}",
                    range.start, range.end
                )));
            }
            if range.end >= total {
                return Err(LumiError::Config(format!(
                    "SHARD_RANGE end {} must be below the fleet total {total}",
                    range.end
                )));
            }
        }

        Ok(())
    }

    /// Returns true if the user ID belongs to a bot owner.
    pub fn is_owner(&self, user_id: u64) -> bool {
        self.owner_ids.contains(&user_id)
    }
}

fn has_scheme(url: &str, schemes: &[&str]) -> bool {
    schemes.iter().any(|scheme| url.trim().starts_with(scheme))
}

fn is_ed25519_hex(key: &str) -> bool {
    let key = key.trim();
    key.len() == 64 && key.bytes().all(|b| b.is_ascii_hexdigit())
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

    const KEY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn config(role: Role) -> Config {
        Config {
            role,
            token: "token".into(),
            database_url: "postgres://localhost/lumi".into(),
            database_read_url: None,
            redis_url: "redis://localhost".into(),
            rpc_token: None,
            rpc_port: 8080,
            owner_ids: Vec::new(),
            custom_dir: "addons".into(),
            shard_count: None,
            shard_range: None,
            intents_guild_members: false,
            discord_public_key: Some(KEY.into()),
            shard_coordinator: false,
            max_shards_per_node: 16,
        }
    }

    #[test]
    fn every_role_accepts_a_complete_config() {
        for role in [Role::All, Role::Gateway, Role::Worker, Role::Api] {
            assert!(
                config(role).validate_for(role).is_ok(),
                "{role:?} rejected a complete config"
            );
        }
    }

    #[test]
    fn gateway_roles_require_a_token() {
        for role in [Role::All, Role::Gateway] {
            let mut cfg = config(role);
            cfg.token = "  ".into();
            assert!(
                cfg.validate_for(role).is_err(),
                "{role:?} accepted no token"
            );
        }

        for role in [Role::Worker, Role::Api] {
            let mut cfg = config(role);
            cfg.token = String::new();
            assert!(cfg.validate_for(role).is_ok(), "{role:?} demanded a token");
        }
    }

    #[test]
    fn api_roles_require_the_interaction_key() {
        for role in [Role::Api, Role::All] {
            let mut cfg = config(role);
            cfg.discord_public_key = None;
            assert!(
                cfg.validate_for(role).is_err(),
                "{role:?} booted without an interaction key"
            );
        }

        let mut cfg = config(Role::Gateway);
        cfg.discord_public_key = None;
        assert!(cfg.validate_for(Role::Gateway).is_ok(), "gateway without public key is valid");
    }

    #[test]
    fn malformed_interaction_key_is_rejected() {
        let keys = [
            String::new(),
            "zz".to_string(),
            "a".repeat(63),
            "a".repeat(65),
        ];
        for key in keys {
            let mut cfg = config(Role::Api);
            cfg.discord_public_key = Some(key.clone());
            assert!(cfg.validate_for(Role::Api).is_err(), "accepted key '{key}'");
        }

        let mut cfg = config(Role::Api);
        cfg.discord_public_key = Some(KEY.to_ascii_uppercase());
        assert!(cfg.validate_for(Role::Api).is_ok(), "valid interaction key accepted");
    }

    #[test]
    fn storage_urls_must_have_supported_schemes() {
        let mut cfg = config(Role::Worker);
        cfg.database_url = "localhost:5432".into();
        assert!(cfg.validate_for(Role::Worker).is_err(), "invalid db url rejected");

        let mut cfg = config(Role::Worker);
        cfg.database_read_url = Some("mysql://localhost".into());
        assert!(cfg.validate_for(Role::Worker).is_err(), "unsupported db scheme rejected");

        let mut cfg = config(Role::Worker);
        cfg.redis_url = "http://localhost".into();
        assert!(cfg.validate_for(Role::Worker).is_err(), "http redis url rejected");
    }

    #[test]
    fn coordinator_requires_fleet_geometry() {
        let mut cfg = config(Role::Gateway);
        cfg.shard_coordinator = true;
        assert!(cfg.validate_for(Role::Gateway).is_err(), "missing shard count rejected");
        cfg.shard_count = Some(8);
        assert!(cfg.validate_for(Role::Gateway).is_ok(), "valid coordinator accepted");
        cfg.max_shards_per_node = 0;
        assert!(cfg.validate_for(Role::Gateway).is_err(), "zero max shards rejected");
    }

    #[test]
    fn deserialized_shard_range_requires_the_fleet_total() {
        let mut cfg = config(Role::Gateway);
        cfg.shard_range = Some(ShardRange { start: 0, end: 3 });
        assert!(cfg.validate_for(Role::Gateway).is_err(), "missing fleet total rejected");
        cfg.shard_count = Some(8);
        assert!(cfg.validate_for(Role::Gateway).is_ok(), "valid shard count accepted");
    }

    #[test]
    fn only_gateway_roles_hold_sessions() {
        assert!(Role::All.is_gateway());
        assert!(Role::Gateway.is_gateway());
        assert!(!Role::Api.is_gateway());
        assert!(!Role::Worker.is_gateway());
    }

    #[test]
    fn zero_fleet_total_is_rejected() {
        let mut cfg = config(Role::Gateway);
        cfg.shard_count = Some(0);
        assert!(cfg.validate_for(Role::Gateway).is_err(), "zero shard count rejected");
    }

    #[test]
    fn deserialized_shard_range_is_checked_again() {
        let mut cfg = config(Role::Gateway);
        cfg.shard_range = Some(ShardRange { start: 4, end: 2 });
        cfg.shard_count = Some(8);
        assert!(cfg.validate_for(Role::Gateway).is_err(), "inverted shard range rejected");

        let mut cfg = config(Role::Gateway);
        cfg.shard_count = Some(4);
        cfg.shard_range = Some(ShardRange { start: 0, end: 4 });
        assert!(cfg.validate_for(Role::Gateway).is_err(), "out of bounds shard range rejected");
    }

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
        assert!(parse_shard_range("4-8", Some(8)).is_err(), "range past fleet total rejected");
        assert!(parse_shard_range("0-7", Some(8)).is_ok(), "valid range accepted");
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
