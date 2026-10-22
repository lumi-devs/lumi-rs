//! Leveling accounts per user and guild.

use crate::ids::{GuildId, UserId};
use sea_orm::entity::prelude::*;

/// Database model for a leveling account.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "level_accounts")]
pub struct Model {
    /// Discord guild snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    /// Discord user snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: UserId,
    /// Current level.
    pub level: i32,
    /// Current experience points in this level.
    pub xp: i64,
    /// Cumulative experience points.
    pub total_xp: i64,
    /// Total text messages tracked.
    pub message_count: i32,
    /// Total voice chat minutes tracked.
    pub voice_minutes: i32,
    /// Timestamp of the last experience gain.
    pub last_xp_at: Option<DateTimeWithTimeZone>,
}

impl ActiveModelBehavior for ActiveModel {}
