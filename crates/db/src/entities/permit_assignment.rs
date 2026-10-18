use sea_orm::entity::prelude::*;
use super::enums;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "permit_assignments")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub permit_id: i32,
    pub guild_id: String,
    pub target_type: enums::PermitTargetType,
    pub target_id: String,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
