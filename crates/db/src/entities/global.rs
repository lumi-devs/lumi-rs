use sea_orm::entity::prelude::*;
use crate::ids::GuildId;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "global_config")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub bot_name: String,
    pub default_prefix: String,
    pub maintenance_mode: bool,
    pub maintenance_message: Option<String>,
    pub invite_url: Option<String>,
    pub support_guild_id: Option<GuildId>,
    pub server_lock_enabled: bool,
    pub server_lock_guild_ids: Vec<i64>,
    pub auto_update_enabled: bool,
    pub auto_update_interval_minutes: i32,
    pub auto_update_last_checked_at: Option<DateTimeWithTimeZone>,
    pub updated_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
