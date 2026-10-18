use sea_orm::entity::prelude::*;
use super::enums;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "audit_ledger")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub guild_id: String,
    pub user_id: String,
    pub action: String,
    pub platform: enums::AuditPlatform,
    pub details: Option<Json>,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
