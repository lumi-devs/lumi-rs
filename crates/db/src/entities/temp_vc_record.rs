//! Active temporary voice channels.

use crate::ids::{ChannelId, GuildId, UserId};
use sea_orm::entity::prelude::*;

/// Database model for an active temporary voice channel.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "tempvc_records")]
pub struct Model {
    /// Discord guild snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    /// Discord voice channel snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub channel_id: ChannelId,
    /// Owner Discord user snowflake.
    pub owner_id: UserId,
    /// Parent generator channel snowflake.
    pub generator_id: ChannelId,
    /// Channel display name.
    pub name: String,
    /// Sequential channel number.
    pub number: i32,
    /// Whether the channel is locked to newcomers.
    pub locked: bool,
    /// Whether the channel is hidden.
    pub hidden: bool,
    /// Timestamp when the channel was created.
    pub created_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
