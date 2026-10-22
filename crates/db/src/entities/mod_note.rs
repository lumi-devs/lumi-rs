//! Staff notes attached to users within a guild.

use crate::ids::{GuildId, UserId};
use sea_orm::entity::prelude::*;

/// Database model for a staff moderation note.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "mod_notes")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i32,
    /// Discord guild snowflake.
    pub guild_id: GuildId,
    /// Target Discord user snowflake.
    pub user_id: UserId,
    /// Author staff member snowflake.
    pub author_id: UserId,
    /// Note content text.
    pub message: String,
    /// Timestamp when the note was created.
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
