use poise::serenity_prelude as serenity;
use serenity::all::{ChannelId, PermissionOverwrite, PermissionOverwriteType, Permissions};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::{Card, CardKind};

/// Security lockdown and panic management.
#[poise::command(
    slash_command,
    guild_only,
    subcommands("on", "off", "status_cmd"),
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn panic(ctx: Context<'_>) -> Result<(), Error> {
    panic_status_action(ctx).await
}

async fn panic_status_action(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let sec = &ctx.data().security;

    let current = sec.get_panic_status(&guild_id.to_string()).await?;

    let card = if let Some(st) = current {
        let started_str = st
            .started_at
            .map(|t| format!("<t:{}:F> (<t:{}:R>)", t.timestamp(), t.timestamp()))
            .unwrap_or_else(|| "Unknown".to_string());

        let lines = vec![
            "**Status**: 🚨 Lockdown Active".to_string(),
            format!("**Actor**: <@{}>", st.actor_id.unwrap_or_else(|| "Unknown".to_string())),
            format!("**Started At**: {started_str}"),
            format!("**Invites Paused**: {}", if st.invites_paused { "Yes" } else { "No" }),
            format!("**Locked Channels Count**: {}", st.locked_channels.len()),
        ];
        Card::lines("Panic Status", lines).kind(CardKind::Error)
    } else {
        let lines = vec![
            "**Status**: 🟢 Normal".to_string(),
            "**Lockdown**: Inactive".to_string(),
            "All channels and features operating normally.".to_string(),
        ];
        Card::lines("Panic Status", lines).kind(CardKind::Success)
    };

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Activate panic mode: pause server invites and lock down text channels.
#[poise::command(slash_command, rename = "on", guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn on(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let actor_id = ctx.author().id;
    let sec = &ctx.data().security;

    if let Some(existing) = sec.get_panic_status(&guild_id.to_string()).await? {
        if existing.active {
            let started_fmt = existing
                .started_at
                .map(|t| format!("<t:{}:R>", t.timestamp()))
                .unwrap_or_else(|| "previously".to_string());
            let card = Card::new(
                "Panic Mode Already Active",
                format!("Panic mode was already triggered {started_fmt} by <@{}>.", existing.actor_id.unwrap_or_default()),
            )
            .kind(CardKind::Warning);
            ctx.send(card.into_reply().reply()).await?;
            return Ok(());
        }
    }

    let channels = guild_id.channels(ctx.http()).await?;
    let mut locked = Vec::new();

    for channel in channels {
        let channel_id = channel.id;
        if channel.is_text_based() {
            let overwrite = PermissionOverwrite {
                allow: Permissions::empty(),
                deny: Permissions::SEND_MESSAGES | Permissions::ADD_REACTIONS,
                kind: PermissionOverwriteType::Role(guild_id.everyone_role()),
            };
            if channel_id.create_permission(ctx.http(), overwrite, Some("Panic mode activated")).await.is_ok() {
                locked.push(channel_id.to_string());
            }
        }
    }

    sec.activate_panic(&guild_id.to_string(), &actor_id.to_string(), true, locked.clone())
        .await?;

    let lines = vec![
        format!("Triggered by <@{}>.", actor_id),
        format!("**Locked Channels**: `{}`", locked.len()),
        "Server invites are paused. Only administrators may send messages.".to_string(),
        "Use `/panic off` when the threat has been resolved.".to_string(),
    ];
    let card = Card::lines("🚨 Panic Lockdown Activated", lines).kind(CardKind::Error);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Deactivate panic mode: restore channel permissions and resume normal activity.
#[poise::command(slash_command, rename = "off", guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn off(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let sec = &ctx.data().security;

    let prior_status = sec.deactivate_panic(&guild_id.to_string()).await?;

    let Some(prior) = prior_status else {
        let card = Card::new(
            "Panic Mode Inactive",
            "Panic mode is not currently active on this server.",
        )
        .kind(CardKind::Warning);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    };

    let mut restored = 0;
    for ch_str in &prior.locked_channels {
        if let Ok(ch_num) = ch_str.parse::<u64>() {
            let ch_id = ChannelId::new(ch_num);
            if ch_id
                .delete_permission(ctx.http(), PermissionOverwriteType::Role(guild_id.everyone_role()), Some("Panic mode deactivated"))
                .await
                .is_ok()
            {
                restored += 1;
            }
        }
    }

    let lines = vec![
        "Normal server operations have been restored.".to_string(),
        format!("**Restored Channels**: `{restored}`"),
        "Members can now send messages and chat freely.".to_string(),
    ];
    let card = Card::lines("🟢 Panic Mode Deactivated", lines).kind(CardKind::Success);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Check current panic lockdown status.
#[poise::command(slash_command, rename = "status", guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn status_cmd(ctx: Context<'_>) -> Result<(), Error> {
    panic_status_action(ctx).await
}

pub fn commands() -> Vec<Command> {
    vec![panic()]
}
