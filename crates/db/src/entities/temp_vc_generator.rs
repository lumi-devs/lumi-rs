use crate::ids::{ChannelId, GuildId};
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "tempvc_generators")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    #[sea_orm(primary_key, auto_increment = false)]
    pub channel_id: ChannelId,
    pub name: String,
    pub limit: i32,
}

impl ActiveModelBehavior for ActiveModel {}
