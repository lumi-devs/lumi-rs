//! Permission definitions per guild.

use super::enums;
use crate::ids::GuildId;
use sea_orm::entity::prelude::*;

/// Database model for a guild permission definition.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "permits")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i32,
    /// Discord guild snowflake.
    #[sea_orm(unique_key = "permits_guild_name")]
    pub guild_id: GuildId,
    /// Permit name.
    #[sea_orm(unique_key = "permits_guild_name")]
    pub name: String,
    /// Permit kind.
    pub kind: enums::PermitKind,
    /// Permit polarity.
    pub polarity: enums::PermitPolarity,
    /// Permission nodes granted or denied by this permit.
    pub nodes: Vec<String>,
    /// Whether this is a builtin default permit.
    pub builtin: bool,
    /// Evaluation sort order position.
    pub position: i32,
    /// Timestamp when the permit was created.
    pub created_at: DateTimeWithTimeZone,
    /// Timestamp when the permit was last updated.
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
