use super::enums;
use crate::ids::{GuildId, SnowflakeId};
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "permit_assignments")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique_key = "permit_assignments_permit_target")]
    pub permit_id: i32,
    pub guild_id: GuildId,
    #[sea_orm(unique_key = "permit_assignments_permit_target")]
    pub target_type: enums::PermitTargetType,
    #[sea_orm(unique_key = "permit_assignments_permit_target")]
    pub target_id: SnowflakeId,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
