//! Generator voice channels for temporary voice channels.

use crate::ids::{ChannelId, GuildId};
use sea_orm::entity::prelude::*;

/// Database model for a temporary voice channel generator.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "tempvc_generators")]
pub struct Model {
    /// Discord guild snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    /// Discord voice channel snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub channel_id: ChannelId,
    /// Default name template for spawned channels.
    pub name: String,
    /// Default user limit for spawned channels.
    pub limit: i32,
}

impl ActiveModelBehavior for ActiveModel {}
