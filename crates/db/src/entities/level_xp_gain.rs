use crate::ids::{GuildId, UserId};
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "level_xp_gains")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub guild_id: GuildId,
    pub user_id: UserId,
    pub amount: i64,
    pub source: String,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
