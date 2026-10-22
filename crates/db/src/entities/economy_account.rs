//! Economy accounts per user and guild.

use crate::ids::{GuildId, UserId};
use sea_orm::entity::prelude::*;

/// Database model for an economy account.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "economy_accounts")]
pub struct Model {
    /// Discord guild snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    /// Discord user snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: UserId,
    /// Wallet balance.
    pub wallet: i64,
    /// Bank balance.
    pub bank: i64,
    /// Timestamp of the last claimed payday.
    pub last_payday_at: Option<DateTimeWithTimeZone>,
}

impl ActiveModelBehavior for ActiveModel {}
