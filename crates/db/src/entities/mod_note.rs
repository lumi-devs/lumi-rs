use sea_orm::entity::prelude::*;
use crate::ids::{GuildId, UserId};

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "mod_notes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub guild_id: GuildId,
    pub user_id: UserId,
    pub author_id: UserId,
    pub message: String,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
