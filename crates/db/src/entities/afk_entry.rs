//! AFK status entries per user and guild.

use crate::ids::{GuildId, UserId};
use sea_orm::entity::prelude::*;

/// Database model for an AFK entry.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "afk_entries")]
pub struct Model {
    /// Discord user snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: UserId,
    /// Discord guild snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    /// AFK reason text.
    pub reason: String,
    /// Timestamp when AFK was set.
    pub since: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
