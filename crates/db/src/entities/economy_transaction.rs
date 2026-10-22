//! Economy ledger transaction records.

use super::enums;
use crate::ids::{GuildId, UserId};
use sea_orm::entity::prelude::*;

/// Database model for an economy transaction.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "economy_transactions")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i32,
    /// Discord guild snowflake.
    pub guild_id: GuildId,
    /// Discord user snowflake.
    pub user_id: UserId,
    /// Transaction kind.
    pub kind: enums::EconomyTxnKind,
    /// Transaction amount.
    pub amount: i64,
    /// Balance after the transaction.
    pub balance_after: i64,
    /// Optional transaction reason.
    pub reason: Option<String>,
    /// Timestamp when the transaction occurred.
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
