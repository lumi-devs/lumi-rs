//! Emergency guild panic lockdown state.

use crate::ids::{GuildId, UserId};
use sea_orm::entity::prelude::*;

/// Database model for emergency panic state.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[sea_orm(table_name = "panic_states")]
pub struct Model {
    /// Discord guild snowflake.
    #[sea_orm(primary_key, auto_increment = false)]
    pub guild_id: GuildId,
    /// User snowflake that triggered the panic state.
    pub actor_id: UserId,
    /// Whether guild invites are paused.
    pub invites_paused: bool,
    /// JSON list of channels locked during panic.
    pub locked_channels: Json,
    /// Timestamp when panic mode was initiated.
    pub started_at: DateTimeWithTimeZone,
}

impl ActiveModelBehavior for ActiveModel {}
