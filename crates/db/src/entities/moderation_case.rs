//! Moderation action cases per guild.

use super::enums;
use crate::ids::{GuildId, UserId};
use sea_orm::entity::prelude::*;

/// Database model for a moderation case.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "moderation_cases")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i32,
    /// Discord guild snowflake.
    #[sea_orm(unique_key = "moderation_cases_guild_case")]
    pub guild_id: GuildId,
    /// Sequential case number within the guild.
    #[sea_orm(unique_key = "moderation_cases_guild_case")]
    pub case_number: i32,
    /// Moderated user snowflake.
    #[sea_orm(indexed)]
    pub user_id: UserId,
    /// Acting moderator user snowflake.
    pub moderator_id: UserId,
    /// Moderation action kind.
    pub action: enums::CaseAction,
    /// Optional custom action name.
    pub custom_action: Option<String>,
    /// Optional action reason.
    pub reason: Option<String>,
    /// Duration in seconds for timed actions.
    pub duration: Option<i32>,
    /// Timestamp when the action expires.
    #[sea_orm(indexed)]
    pub expires_at: Option<DateTimeWithTimeZone>,
    /// Whether the case is currently active.
    pub active: bool,
    /// Timestamp when the case was created.
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
