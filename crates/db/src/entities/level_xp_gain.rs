//! History of individual leveling experience gains.

use crate::ids::{GuildId, UserId};
use sea_orm::entity::prelude::*;

/// Database model for a leveling experience gain event.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "level_xp_gains")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i32,
    /// Discord guild snowflake.
    pub guild_id: GuildId,
    /// Discord user snowflake.
    pub user_id: UserId,
    /// Amount of experience gained.
    pub amount: i64,
    /// Source of the experience gain.
    pub source: String,
    /// Timestamp when the experience was awarded.
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
