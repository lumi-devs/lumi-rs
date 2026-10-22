//! Named per-guild monotonic counters.

use crate::ids::GuildId;
use sea_orm::entity::prelude::*;

/// Database model for a named per-guild counter.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "counters")]
pub struct Model {
    /// Discord guild snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    /// Counter name.
    #[sea_orm(primary_key, auto_increment = false)]
    pub name: String,
    /// Current counter value.
    pub value: i64,
}

impl ActiveModelBehavior for ActiveModel {}
