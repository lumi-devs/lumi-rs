use sea_orm::entity::prelude::*;

use crate::permits::PermitTargetType;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "permit_assignments")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique_key = "uq_permit_assignment")]
    pub permit_id: i32,
    #[sea_orm(indexed)]
    pub guild_id: i64,
    #[sea_orm(unique_key = "uq_permit_assignment")]
    pub target_type: PermitTargetType,
    #[sea_orm(unique_key = "uq_permit_assignment")]
    pub target_id: i64,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
