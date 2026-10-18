use sea_orm::entity::prelude::*;
use super::enums;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "warn_thresholds")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub warn_count: i32,
    pub action: enums::CaseAction,
    pub duration: Option<i32>,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
