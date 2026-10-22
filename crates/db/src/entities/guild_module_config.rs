//! Per-guild module configuration values.

use crate::ids::GuildId;
use sea_orm::entity::prelude::*;

/// Database model for per-guild module configuration.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "guild_module_config")]
pub struct Model {
    /// Discord guild snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    /// Module name.
    #[sea_orm(primary_key, auto_increment = false)]
    pub module_name: String,
    /// Configuration setting key.
    #[sea_orm(primary_key, auto_increment = false)]
    pub config_key: String,
    /// JSON configuration payload.
    pub value: Json,
    /// Timestamp when the configuration was last updated.
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
