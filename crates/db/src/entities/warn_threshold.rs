//! Escalation actions triggered at specific warning counts.

use super::enums;
use crate::ids::GuildId;
use sea_orm::entity::prelude::*;

/// Database model for a warning escalation threshold.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "warn_thresholds")]
pub struct Model {
    /// Discord guild snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    /// Warning count required to trigger action.
    #[sea_orm(primary_key, auto_increment = false)]
    pub warn_count: i32,
    /// Escalation action to take.
    pub action: enums::CaseAction,
    /// Duration in seconds for timed actions.
    pub duration: Option<i32>,
    /// Timestamp when the threshold was created.
    pub created_at: DateTimeWithTimeZone,
    /// Timestamp when the threshold was last updated.
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
