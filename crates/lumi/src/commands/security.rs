use poise::serenity_prelude as serenity;
use serenity::all::{ChannelId, PermissionOverwrite, PermissionOverwriteType, Permissions};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::{Card, CardKind};

/// Security lockdown and panic management.
#[poise::command(
    slash_command,
    guild_only,
    subcommands("start", "revert", "status"),
    default_member_permissions = "ADMINISTRATOR"
)]
pub async fn panic(ctx: Context<'_>) -> Result<(), Error> {
    start(ctx).await
}

/// Activate panic mode: pause invites and lock down text channels.
#[poise::command(slash_command, rename = "start", guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn start(ctx: Context<'_>) -> Result<(), Error> {
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

    for (channel_id, channel) in channels {
        if channel.is_text_based() && !channel.is_thread() {
            let everyone_role = guild_id.everyone_role();
            let overwrite = PermissionOverwrite {
                allow: Permissions::empty(),
                deny: Permissions::SEND_MESSAGES,
                kind: PermissionOverwriteType::Role(everyone_role),
            };
            if channel.create_permission(ctx.http(), overwrite).await.is_ok() {
                locked.push(channel_id.to_string());
            }
        }
    }

    let status = sec
        .activate_panic(&guild_id.to_string(), &actor_id.to_string(), true, locked.clone())
        .await?;

    let lines = vec![
        "**Status**: 🚨 Active".to_string(),
        format!("**Triggered by**: <@{actor_id}>"),
        format!("**Invites Paused**: {}", if status.invites_paused { "Yes" } else { "No" }),
        format!("**Channels Locked**: {}", locked.len()),
    ];

    let card = Card::lines("🚨 Panic Lockdown Activated", lines).kind(CardKind::Error);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Revert panic mode: restore channel permissions and unpause invites.
#[poise::command(slash_command, rename = "revert", guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn revert(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let sec = &ctx.data().security;

    let prior_status = sec.deactivate_panic(&guild_id.to_string()).await?;
    let Some(prior) = prior_status else {
        let card = Card::new(
            "Panic Mode Inactive",
            "Panic mode is not currently active on this server.",
        )
        .kind(CardKind::Info);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    };

    let mut restored_count = 0;
    for ch_id_str in prior.locked_channels {
        if let Ok(ch_id) = ch_id_str.parse::<u64>() {
            let channel_id = ChannelId::new(ch_id);
            let everyone_role = guild_id.everyone_role();
            if channel_id
                .delete_permission(ctx.http(), PermissionOverwriteType::Role(everyone_role))
                .await
                .is_ok()
            {
                restored_count += 1;
            }
        }
    }

    let lines = vec![
        "**Status**: ✅ Restored".to_string(),
        format!("**Channels Restored**: {restored_count}"),
        "**Invites**: Resumed".to_string(),
    ];

    let card = Card::lines("Panic Mode Reverted", lines).kind(CardKind::Success);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Revert panic mode via alias.
#[poise::command(slash_command, guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn unpanic(ctx: Context<'_>) -> Result<(), Error> {
    revert(ctx).await
}

/// Check current panic lockdown status.
#[poise::command(slash_command, rename = "status", guild_only, default_member_permissions = "ADMINISTRATOR")]
pub async fn status(ctx: Context<'_>) -> Result<(), Error> {
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
        ];
        Card::lines("Panic Status", lines).kind(CardKind::Success)
    };

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

pub fn commands() -> Vec<Command> {
    vec![panic(), unpanic()]
}
