//! Permit bindings to users, roles, or channels.

use super::enums;
use crate::ids::{GuildId, SnowflakeId};
use sea_orm::entity::prelude::*;

/// Database model for a permit assignment.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "permit_assignments")]
pub struct Model {
    /// Primary key.
    #[sea_orm(primary_key)]
    pub id: i32,
    /// Assigned permit identifier.
    #[sea_orm(unique_key = "permit_assignments_permit_target")]
    pub permit_id: i32,
    /// Discord guild snowflake.
    pub guild_id: GuildId,
    /// Target entity type.
    #[sea_orm(unique_key = "permit_assignments_permit_target")]
    pub target_type: enums::PermitTargetType,
    /// Target snowflake identifier.
    #[sea_orm(unique_key = "permit_assignments_permit_target")]
    pub target_id: SnowflakeId,
    /// Timestamp when the assignment was created.
    pub created_at: DateTimeWithTimeZone,
    /// Timestamp when the assignment was last updated.
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
