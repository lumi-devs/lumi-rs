use sea_orm::entity::prelude::*;
use super::enums;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "moderation_cases")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub guild_id: String,
    pub case_number: i32,
    pub user_id: String,
    pub moderator_id: String,
    pub action: enums::CaseAction,
    pub custom_action: Option<String>,
    pub reason: Option<String>,
    pub duration: Option<i32>,
    pub expires_at: Option<DateTimeWithTimeZone>,
    pub active: bool,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
