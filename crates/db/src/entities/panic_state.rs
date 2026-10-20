use crate::ids::{GuildId, UserId};
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "panic_states")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    pub actor_id: UserId,
    pub invites_paused: bool,
    pub locked_channels: Json,
    pub started_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
