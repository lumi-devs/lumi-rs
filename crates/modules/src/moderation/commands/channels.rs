//! Channel and server lockdown commands.

use serenity::all::{
    ChannelId, GenericChannelId, PermissionOverwrite, PermissionOverwriteType, RoleId,
};

use lumi_kernel::bot::{Context, Error};
use lumi_ui::{Card, CardKind};

use crate::moderation::channels as permissions;

use super::{guild_id, send_card};

/// The channel's current `@everyone` overwrite, so locking merges instead of
/// replacing unrelated permission bits.
async fn everyone_overwrite(
    ctx: Context<'_>,
    channel_id: ChannelId,
    everyone: RoleId,
) -> Result<Option<PermissionOverwrite>, Error> {
    let channel = channel_id.widen().to_channel(ctx, None).await?;
    Ok(channel.guild().and_then(|channel| {
        channel
            .permission_overwrites
            .iter()
            .find(|overwrite| {
                matches!(overwrite.kind, PermissionOverwriteType::Role(role) if role == everyone)
            })
            .cloned()
    }))
}

/// Lock or unlock one channel for @everyone.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MANAGE_CHANNELS",
    required_bot_permissions = "MANAGE_CHANNELS",
    subcommands("lock_enable", "lock_disable")
)]
pub async fn lock(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    send_card(
        ctx,
        Card::lines(
            "🔒 Lock",
            vec!["Use `/lock enable` or `/lock disable`.".to_string()],
        )
        .kind(CardKind::Info),
    )
    .await
}

/// Deny @everyone `SendMessages` in a channel.
#[poise::command(
    slash_command,
    prefix_command,
    rename = "enable",
    guild_only,
    default_member_permissions = "MANAGE_CHANNELS",
    required_bot_permissions = "MANAGE_CHANNELS"
)]
pub async fn lock_enable(
    ctx: Context<'_>,
    #[description = "Channel to lock (defaults to this one)"] channel: Option<GenericChannelId>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = guild_id(ctx)?;
    let channel_id = ChannelId::new(channel.unwrap_or_else(|| ctx.channel_id()).get());
    let existing = everyone_overwrite(ctx, channel_id, guild.everyone_role()).await?;

    permissions::set_channel_locked(
        ctx.http(),
        channel_id,
        guild.everyone_role(),
        existing.as_ref(),
        true,
        "Channel locked",
    )
    .await?;

    send_card(
        ctx,
        Card::lines(
            "🔒 Channel Locked",
            vec![format!(
                "<#{}> is now locked - @everyone can no longer send messages.",
                channel_id.get()
            )],
        )
        .kind(CardKind::Success),
    )
    .await
}

/// Restore @everyone `SendMessages` in a channel.
#[poise::command(
    slash_command,
    prefix_command,
    rename = "disable",
    guild_only,
    default_member_permissions = "MANAGE_CHANNELS",
    required_bot_permissions = "MANAGE_CHANNELS"
)]
pub async fn lock_disable(
    ctx: Context<'_>,
    #[description = "Channel to unlock (defaults to this one)"] channel: Option<GenericChannelId>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = guild_id(ctx)?;
    let channel_id = ChannelId::new(channel.unwrap_or_else(|| ctx.channel_id()).get());
    let existing = everyone_overwrite(ctx, channel_id, guild.everyone_role()).await?;

    permissions::set_channel_locked(
        ctx.http(),
        channel_id,
        guild.everyone_role(),
        existing.as_ref(),
        false,
        "Channel unlocked",
    )
    .await?;

    send_card(
        ctx,
        Card::lines(
            "🔓 Channel Unlocked",
            vec![format!(
                "<#{}> is now unlocked - @everyone can send messages again.",
                channel_id.get()
            )],
        )
        .kind(CardKind::Success),
    )
    .await
}

/// Enable or disable server-wide channel lockdown.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "ADMINISTRATOR",
    required_bot_permissions = "MANAGE_CHANNELS",
    subcommands("lockdown_enable", "lockdown_disable")
)]
pub async fn lockdown(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    send_card(
        ctx,
        Card::lines(
            "🔒 Lockdown",
            vec!["Use `/lockdown enable` or `/lockdown disable`.".to_string()],
        )
        .kind(CardKind::Info),
    )
    .await
}

/// Lock down every text channel for @everyone.
#[poise::command(
    slash_command,
    prefix_command,
    rename = "enable",
    guild_only,
    default_member_permissions = "ADMINISTRATOR",
    required_bot_permissions = "MANAGE_CHANNELS"
)]
pub async fn lockdown_enable(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = guild_id(ctx)?;
    let changes = permissions::set_guild_locked(ctx.http(), guild, true, "Server lockdown").await;

    if changes.all_failed() {
        return send_card(
            ctx,
            Card::lines(
                "❌ Lockdown Failed",
                vec![format!(
                    "Could not modify permissions for {} channel(s).",
                    changes.failed
                )],
            )
            .kind(CardKind::Error),
        )
        .await;
    }

    let mut lines = vec![format!(
        "Disabled SendMessages for @everyone in {} text channel(s).",
        changes.modified
    )];
    if changes.failed > 0 {
        lines.push(format!(
            "{} channel(s) could not be modified.",
            changes.failed
        ));
    }
    send_card(
        ctx,
        Card::lines("🔒 Lockdown Enabled", lines).kind(CardKind::Success),
    )
    .await
}

/// Lift the server-wide channel lockdown.
#[poise::command(
    slash_command,
    prefix_command,
    rename = "disable",
    guild_only,
    default_member_permissions = "ADMINISTRATOR",
    required_bot_permissions = "MANAGE_CHANNELS"
)]
pub async fn lockdown_disable(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = guild_id(ctx)?;
    let changes =
        permissions::set_guild_locked(ctx.http(), guild, false, "Server lockdown lifted").await;

    if changes.all_failed() {
        return send_card(
            ctx,
            Card::lines(
                "❌ Lockdown Disable Failed",
                vec![format!(
                    "Could not modify permissions for {} channel(s).",
                    changes.failed
                )],
            )
            .kind(CardKind::Error),
        )
        .await;
    }

    let mut lines = vec![format!(
        "Restored SendMessages for @everyone in {} text channel(s).",
        changes.modified
    )];
    if changes.failed > 0 {
        lines.push(format!(
            "{} channel(s) could not be modified.",
            changes.failed
        ));
    }
    send_card(
        ctx,
        Card::lines("🔓 Lockdown Disabled", lines).kind(CardKind::Success),
    )
    .await
}
