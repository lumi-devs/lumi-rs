use super::enums;
use crate::ids::{GuildId, UserId};
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "moderation_cases")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    #[sea_orm(unique_key = "moderation_cases_guild_case")]
    pub guild_id: GuildId,
    #[sea_orm(unique_key = "moderation_cases_guild_case")]
    pub case_number: i32,
    #[sea_orm(indexed)]
    pub user_id: UserId,
    pub moderator_id: UserId,
    pub action: enums::CaseAction,
    pub custom_action: Option<String>,
    pub reason: Option<String>,
    pub duration: Option<i32>,
    #[sea_orm(indexed)]
    pub expires_at: Option<DateTimeWithTimeZone>,
    pub active: bool,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
