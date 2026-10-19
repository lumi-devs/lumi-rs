//! Shared channel-wide `@everyone` permission flow used by manual lockdown
//! and the filter module's automatic lockdown.

use poise::serenity_prelude as serenity;
use serenity::all::{
    ChannelId, ChannelType, GuildId, Http, PermissionOverwrite, PermissionOverwriteType,
    Permissions,
};

/// How many text channels were changed or rejected.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChannelChanges {
    pub modified: u32,
    pub failed: u32,
}

impl ChannelChanges {
    #[must_use]
    pub fn all_failed(self) -> bool {
        self.modified == 0 && self.failed > 0
    }
}

/// Apply or remove the `@everyone` SendMessages denial for one channel.
pub async fn set_channel_locked(
    http: &Http,
    channel_id: ChannelId,
    everyone: serenity::all::RoleId,
    locked: bool,
    reason: &str,
) -> Result<(), serenity::Error> {
    if locked {
        channel_id
            .create_permission(
                http,
                PermissionOverwrite {
                    allow: Permissions::empty(),
                    deny: Permissions::SEND_MESSAGES,
                    kind: PermissionOverwriteType::Role(everyone),
                },
                Some(reason),
            )
            .await
    } else {
        channel_id
            .delete_permission(http, PermissionOverwriteType::Role(everyone), Some(reason))
            .await
    }
}

/// Lock or unlock every text/news channel in a guild for `@everyone`.
pub async fn set_guild_locked(
    http: &Http,
    guild_id: GuildId,
    locked: bool,
    reason: &str,
) -> ChannelChanges {
    let Ok(channels) = guild_id.channels(http).await else {
        tracing::warn!(guild_id = guild_id.get(), "could not fetch channels for lockdown");
        return ChannelChanges {
            modified: 0,
            failed: 1,
        };
    };

    let everyone = guild_id.everyone_role();
    let mut changes = ChannelChanges::default();
    for channel in channels.iter() {
        if !matches!(channel.base.kind, ChannelType::Text | ChannelType::News) {
            continue;
        }
        match set_channel_locked(http, channel.id, everyone, locked, reason).await {
            Ok(()) => changes.modified = changes.modified.saturating_add(1),
            Err(error) => {
                tracing::debug!(%error, channel_id = channel.id.get(), "channel lockdown failed");
                changes.failed = changes.failed.saturating_add(1);
            }
        }
    }
    changes
}
