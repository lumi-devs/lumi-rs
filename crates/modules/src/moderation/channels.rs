//! Shared channel-wide `@everyone` permission flow used by manual lockdown
//! and the filter module's automatic lockdown.

use poise::serenity_prelude as serenity;
use serenity::all::{
    ChannelId, ChannelType, GuildId, Http, PermissionOverwrite, PermissionOverwriteType,
    Permissions, RoleId,
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

/// Merge a deny mask into the `@everyone` overwrite while preserving every
/// unrelated bit. Locking must not wipe a channel's existing view/reaction
/// restrictions, and unlocking must not publish a private channel. `None`
/// means the overwrite should be deleted.
#[must_use]
pub fn merge_everyone_overwrite(
    existing: Option<&PermissionOverwrite>,
    everyone: RoleId,
    mask: Permissions,
    locked: bool,
) -> Option<PermissionOverwrite> {
    let allow = existing.map_or(Permissions::empty(), |overwrite| overwrite.allow);
    let mut deny = existing.map_or(Permissions::empty(), |overwrite| overwrite.deny);
    if locked {
        deny |= mask;
    } else {
        deny.remove(mask);
    }
    if !locked && allow.is_empty() && deny.is_empty() {
        return None;
    }
    Some(PermissionOverwrite {
        allow,
        deny,
        kind: PermissionOverwriteType::Role(everyone),
    })
}

/// Apply or remove the `@everyone` SendMessages denial for one channel.
pub async fn set_channel_locked(
    http: &Http,
    channel_id: ChannelId,
    everyone: RoleId,
    existing: Option<&PermissionOverwrite>,
    locked: bool,
    reason: &str,
) -> Result<(), serenity::Error> {
    match merge_everyone_overwrite(existing, everyone, Permissions::SEND_MESSAGES, locked) {
        Some(overwrite) => {
            channel_id
                .create_permission(http, overwrite, Some(reason))
                .await
        }
        None => {
            channel_id
                .delete_permission(http, PermissionOverwriteType::Role(everyone), Some(reason))
                .await
        }
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
        tracing::warn!(
            guild_id = guild_id.get(),
            "could not fetch channels for lockdown"
        );
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
        let existing = channel
            .permission_overwrites
            .iter()
            .find(|overwrite| matches!(overwrite.kind, PermissionOverwriteType::Role(role) if role == everyone));
        match set_channel_locked(http, channel.id, everyone, existing, locked, reason).await {
            Ok(()) => changes.modified = changes.modified.saturating_add(1),
            Err(error) => {
                tracing::debug!(%error, channel_id = channel.id.get(), "channel lockdown failed");
                changes.failed = changes.failed.saturating_add(1);
            }
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::merge_everyone_overwrite;
    use serenity::all::{PermissionOverwrite, PermissionOverwriteType, Permissions, RoleId};

    fn overwrite(allow: Permissions, deny: Permissions) -> PermissionOverwrite {
        PermissionOverwrite {
            allow,
            deny,
            kind: PermissionOverwriteType::Role(RoleId::new(1)),
        }
    }

    #[test]
    fn locking_preserves_unrelated_overwrites() {
        let everyone = RoleId::new(1);
        let existing = overwrite(Permissions::empty(), Permissions::VIEW_CHANNEL);

        let locked =
            merge_everyone_overwrite(Some(&existing), everyone, Permissions::SEND_MESSAGES, true)
                .expect("overwrite stays");
        assert_eq!(
            locked.deny,
            Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES
        );
    }

    #[test]
    fn unlocking_restores_the_previous_overwrite() {
        let everyone = RoleId::new(1);
        let existing = overwrite(
            Permissions::empty(),
            Permissions::VIEW_CHANNEL | Permissions::SEND_MESSAGES,
        );

        let unlocked =
            merge_everyone_overwrite(Some(&existing), everyone, Permissions::SEND_MESSAGES, false)
                .expect("view restriction must survive");
        assert_eq!(unlocked.deny, Permissions::VIEW_CHANNEL);
        assert!(unlocked.allow.is_empty());
    }

    #[test]
    fn locking_an_untouched_channel_is_fully_reverted() {
        let everyone = RoleId::new(1);
        let locked = merge_everyone_overwrite(None, everyone, Permissions::SEND_MESSAGES, true)
            .expect("lock creates an overwrite");
        assert_eq!(locked.deny, Permissions::SEND_MESSAGES);

        assert!(
            merge_everyone_overwrite(Some(&locked), everyone, Permissions::SEND_MESSAGES, false)
                .is_none(),
            "unlock deletes the overwrite it created"
        );
    }

    #[test]
    fn explicit_allow_survives_lock_and_unlock() {
        let everyone = RoleId::new(1);
        let existing = overwrite(Permissions::SEND_MESSAGES, Permissions::empty());
        let locked =
            merge_everyone_overwrite(Some(&existing), everyone, Permissions::SEND_MESSAGES, true)
                .expect("lock keeps the allow");
        assert_eq!(locked.allow, Permissions::SEND_MESSAGES);
        assert_eq!(locked.deny, Permissions::SEND_MESSAGES);

        let unlocked =
            merge_everyone_overwrite(Some(&locked), everyone, Permissions::SEND_MESSAGES, false)
                .expect("allow remains");
        assert_eq!(unlocked.allow, Permissions::SEND_MESSAGES);
        assert!(unlocked.deny.is_empty());
    }
}
