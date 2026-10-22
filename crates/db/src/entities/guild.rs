//! Guild settings and metadata.

use crate::ids::GuildId;
use sea_orm::entity::prelude::*;

/// Database model for a Discord guild.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "guilds")]
pub struct Model {
    /// Discord guild snowflake.
    #[sea_orm(primary_key)]
    pub id: GuildId,
    /// Custom command prefix override.
    pub prefix: Option<String>,
    /// Configured locale string.
    pub locale: String,
    /// Whether the guild is ignored by the bot.
    pub ignored: bool,
    /// Timestamp when the bot left the guild.
    #[sea_orm(indexed)]
    pub left_at: Option<DateTimeWithTimeZone>,
    /// Timestamp when the guild was registered.
    pub created_at: DateTimeWithTimeZone,
    /// Timestamp when the guild record was last updated.
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
