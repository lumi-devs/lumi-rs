use crate::ids::{GuildId, UserId};
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "afk_entries")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub user_id: UserId,
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    pub reason: String,
    pub since: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
