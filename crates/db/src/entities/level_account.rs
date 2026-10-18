use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "level_accounts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: String,
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: String,
    pub level: i32,
    pub xp: i64,
    pub total_xp: i64,
    pub message_count: i32,
    pub voice_minutes: i32,
    pub last_xp_at: Option<DateTimeWithTimeZone>,
}

impl ActiveModelBehavior for ActiveModel {}
