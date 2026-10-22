//! Global bot configuration.

use crate::ids::GuildId;
use sea_orm::entity::prelude::*;

/// Database model for global bot configuration.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "global_config")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i32,
    /// Configured bot display name.
    pub bot_name: String,
    /// Default command prefix.
    pub default_prefix: String,
    /// Whether global maintenance mode is active.
    pub maintenance_mode: bool,
    /// Optional custom maintenance message.
    pub maintenance_message: Option<String>,
    /// Optional bot invite URL.
    pub invite_url: Option<String>,
    /// Support guild snowflake.
    pub support_guild_id: Option<GuildId>,
    /// Whether server lock is enabled.
    pub server_lock_enabled: bool,
    /// List of guild snowflakes permitted when server lock is active.
    pub server_lock_guild_ids: Vec<i64>,
    /// Whether automatic updates are enabled.
    pub auto_update_enabled: bool,
    /// Auto-update check interval in minutes.
    pub auto_update_interval_minutes: i32,
    /// Timestamp when auto-updates were last checked.
    pub auto_update_last_checked_at: Option<DateTimeWithTimeZone>,
    /// Timestamp when configuration was last updated.
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
