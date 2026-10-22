//! Per-guild module enable/disable states.

use crate::ids::GuildId;
use sea_orm::entity::prelude::*;

/// Database model for per-guild module state.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "guild_module_state")]
pub struct Model {
    /// Discord guild snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    /// Module name.
    #[sea_orm(primary_key, auto_increment = false)]
    pub module_name: String,
    /// Whether the module is enabled in this guild.
    pub enabled: bool,
    /// Timestamp when the module state was last updated.
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
