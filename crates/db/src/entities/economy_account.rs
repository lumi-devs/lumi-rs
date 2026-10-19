use sea_orm::entity::prelude::*;
use crate::ids::{GuildId, UserId};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "economy_accounts")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: UserId,
    pub wallet: i64,
    pub bank: i64,
    pub last_payday_at: Option<DateTimeWithTimeZone>,
}

impl ActiveModelBehavior for ActiveModel {}
