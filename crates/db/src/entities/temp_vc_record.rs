use crate::ids::{ChannelId, GuildId, UserId};
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "tempvc_records")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    #[sea_orm(primary_key, auto_increment = false)]
    pub channel_id: ChannelId,
    pub owner_id: UserId,
    pub generator_id: ChannelId,
    pub name: String,
    pub number: i32,
    pub locked: bool,
    pub hidden: bool,
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
