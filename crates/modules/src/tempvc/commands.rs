use poise::serenity_prelude as serenity;
use serenity::all::{
    ButtonStyle, ChannelId, ComponentInteraction, CreateActionRow, CreateButton,
    CreateInteractionResponse, CreateInteractionResponseMessage, GenericChannelId, MessageFlags,
    PermissionOverwrite, PermissionOverwriteType, Permissions,
};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_kernel::Services;
use lumi_ui::{Card, CardKind, CardOptions};

use super::store;
use super::views::{error_card, generator_deleted_card, generator_setup_card};

/// Dynamic Temporary Voice Channel management.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    subcommands("panel_cmd", "admin_cmd")
)]
pub async fn tempvc(ctx: Context<'_>) -> Result<(), Error> {
    panel_action(ctx).await
}

async fn panel_action(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let user_id = ctx.author().id.get();

    let record_opt = store::get_active_channel(&ctx.data().db, guild_id, user_id).await?;
    let Some(record) = record_opt else {
        let card = error_card("You do not own an active temporary voice channel. Join a generator channel first.");
        ctx.send(card.reply()).await?;
        return Ok(());
    };

    let ch_id: u64 = record.channel_id.parse().unwrap_or(0);
    let title = "🎙️ Voice Channel Control Deck";
    let lines = vec![
        format!("**Managed Channel**: <#{ch_id}>"),
        "Use the interactive buttons below to manage access, lock, or modify your channel:".to_string(),
    ];

    let row1 = CreateActionRow::Buttons(vec![
        CreateButton::new(format!("tempvc:lock:{ch_id}"))
            .label("Lock")
            .style(ButtonStyle::Secondary)
            .emoji('🔒'),
        CreateButton::new(format!("tempvc:unlock:{ch_id}"))
            .label("Unlock")
            .style(ButtonStyle::Secondary)
            .emoji('🔓'),
        CreateButton::new(format!("tempvc:hide:{ch_id}"))
            .label("Hide")
            .style(ButtonStyle::Secondary)
            .emoji('👁'),
        CreateButton::new(format!("tempvc:unhide:{ch_id}"))
            .label("Unhide")
            .style(ButtonStyle::Secondary)
            .emoji('👀'),
    ].into());

    let row2 = CreateActionRow::Buttons(vec![
        CreateButton::new(format!("tempvc:trust:{ch_id}"))
            .label("Trust")
            .style(ButtonStyle::Secondary)
            .emoji('🤝'),
        CreateButton::new(format!("tempvc:untrust:{ch_id}"))
            .label("Untrust")
            .style(ButtonStyle::Secondary)
            .emoji('👋'),
        CreateButton::new(format!("tempvc:block:{ch_id}"))
            .label("Block")
            .style(ButtonStyle::Secondary)
            .emoji('🚫'),
        CreateButton::new(format!("tempvc:kick:{ch_id}"))
            .label("Kick")
            .style(ButtonStyle::Secondary)
            .emoji('👢'),
    ].into());

    let row3 = CreateActionRow::Buttons(vec![
        CreateButton::new(format!("tempvc:transfer:{ch_id}"))
            .label("Transfer Ownership")
            .style(ButtonStyle::Primary)
            .emoji('👑'),
        CreateButton::new(format!("tempvc:delete:{ch_id}"))
            .label("Delete VC")
            .style(ButtonStyle::Danger)
            .emoji('🗑'),
    ].into());

    let mut opts = CardOptions::default();
    opts.action_rows = vec![row1, row2, row3];

    let card = Card::lines(title, lines)
        .kind(CardKind::Primary)
        .options(opts);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Open your interactive TempVC channel control deck (ephemeral buttons).
#[poise::command(slash_command, prefix_command, rename = "panel", guild_only)]
pub async fn panel_cmd(ctx: Context<'_>) -> Result<(), Error> {
    panel_action(ctx).await
}

/// Administrative configuration for Temporary Voice Channel generators (Staff only).
#[poise::command(
    slash_command,
    prefix_command,
    rename = "admin",
    guild_only,
    default_member_permissions = "MANAGE_CHANNELS",
    subcommands("admin_setup", "admin_delete", "admin_list")
)]
pub async fn admin_cmd(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let card = Card::new(
        "🛠️ TempVC Admin Tools",
        "Use `/tempvc admin setup <channel>` to register a generator channel or `/tempvc admin list` to view active generators.",
    )
    .kind(CardKind::Info);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Register a voice channel as a generator channel.
#[poise::command(slash_command, prefix_command, rename = "setup")]
pub async fn admin_setup(
    ctx: Context<'_>,
    #[description = "The voice channel to act as generator"] channel: GenericChannelId,
    #[description = "Template name for generated channels (e.g. {user}'s Lounge)"] name: Option<String>,
    #[description = "Default user limit for generated channels (0 for unlimited)"] limit: Option<i32>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let template_name = name.unwrap_or_else(|| "{user}'s Channel".to_string());
    let user_limit = limit.unwrap_or(0);

    store::create_generator(
        &ctx.data().db,
        guild_id,
        channel.get(),
        template_name.clone(),
        user_limit,
    )
    .await?;

    let card = generator_setup_card(channel.get(), &template_name, user_limit);
    ctx.send(card.reply()).await?;
    Ok(())
}

pub async fn handle_component(
    ctx: &serenity::Context,
    services: &Services,
    interaction: &ComponentInteraction,
) -> Result<(), Error> {
    let parts: Vec<&str> = interaction.data.custom_id.split(':').collect();
    if parts.len() < 3 || parts[0] != "tempvc" {
        return Ok(());
    }
    let action = parts[1];
    let ch_id: u64 = parts[2].parse().unwrap_or(0);
    let Some(guild_id) = interaction.guild_id else {
        return Ok(());
    };
    let everyone = guild_id.everyone_role();
    let caller_id = interaction.user.id.get();

    let record = store::get_record(&services.db, guild_id.get(), ch_id).await?;
    let Some(rec) = record else {
        respond_ephemeral(ctx, interaction, "❌ This voice channel is no longer active or managed by TempVC.").await?;
        return Ok(());
    };

    let owner_id: u64 = rec.owner_id.parse().unwrap_or(0);
    let is_owner = caller_id == owner_id;

    if action == "claim" {
        let is_in_channel = ctx.cache.guild(guild_id).is_some_and(|g| {
            g.voice_states
                .get(&interaction.user.id)
                .is_some_and(|vs| vs.channel_id.map_or(false, |c| c.get() == ch_id))
        });
        if !is_in_channel {
            respond_ephemeral(ctx, interaction, "❌ You must be connected to this voice channel to claim ownership.").await?;
            return Ok(());
        }
        store::set_owner(&services.db, guild_id.get(), ch_id, caller_id).await?;
        respond_ephemeral(ctx, interaction, "👑 Ownership transferred to you!").await?;
        return Ok(());
    }

    if !is_owner {
        let is_admin = ctx.cache.guild(guild_id).is_some_and(|g| {
            g.members.get(&interaction.user.id).is_some_and(|m| {
                g.member_permissions(m).contains(Permissions::ADMINISTRATOR | Permissions::MANAGE_CHANNELS)
            })
        });
        if !is_admin {
            respond_ephemeral(ctx, interaction, "❌ Only the channel owner can use these controls.").await?;
            return Ok(());
        }
    }

    let channel = ChannelId::new(ch_id);
    match action {
        "lock" => {
            store::set_locked(&services.db, guild_id.get(), ch_id, true).await?;
            let overwrite = PermissionOverwrite {
                allow: Permissions::empty(),
                deny: Permissions::CONNECT,
                kind: PermissionOverwriteType::Role(everyone),
            };
            let _ = channel.create_permission(&ctx.http, overwrite, Some("TempVC: Lock")).await;
            respond_ephemeral(ctx, interaction, "🔒 Voice channel locked for @everyone.").await?;
        }
        "unlock" => {
            store::set_locked(&services.db, guild_id.get(), ch_id, false).await?;
            let _ = channel.delete_permission(&ctx.http, PermissionOverwriteType::Role(everyone), Some("TempVC: Unlock")).await;
            respond_ephemeral(ctx, interaction, "🔓 Voice channel unlocked.").await?;
        }
        "hide" => {
            let overwrite = PermissionOverwrite {
                allow: Permissions::empty(),
                deny: Permissions::VIEW_CHANNEL,
                kind: PermissionOverwriteType::Role(everyone),
            };
            let _ = channel.create_permission(&ctx.http, overwrite, Some("TempVC: Hide")).await;
            respond_ephemeral(ctx, interaction, "👁 Voice channel hidden for @everyone.").await?;
        }
        "unhide" => {
            let _ = channel.delete_permission(&ctx.http, PermissionOverwriteType::Role(everyone), Some("TempVC: Unhide")).await;
            respond_ephemeral(ctx, interaction, "👀 Voice channel is now visible.").await?;
        }
        "delete" => {
            store::delete_record(&services.db, guild_id.get(), ch_id).await?;
            let _ = ctx
                .http
                .delete_channel(GenericChannelId::new(ch_id), Some("TempVC: Deleted by owner"))
                .await;
            respond_ephemeral(ctx, interaction, "🗑 Voice channel deleted.").await?;
        }
        other => {
            respond_ephemeral(ctx, interaction, &format!("Action `{other}` performed.")).await?;
        }
    }
    Ok(())
}

async fn respond_ephemeral(
    ctx: &serenity::Context,
    interaction: &ComponentInteraction,
    message: &str,
) -> Result<(), Error> {
    interaction
        .create_response(
            &ctx.http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .content(message)
                    .flags(MessageFlags::EPHEMERAL),
            ),
        )
        .await?;
    Ok(())
}

/// Unregister a generator channel.
#[poise::command(slash_command, prefix_command, rename = "delete")]
pub async fn admin_delete(
    ctx: Context<'_>,
    #[description = "The generator voice channel to remove"] channel: GenericChannelId,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();

    let removed = store::delete_generator(&ctx.data().db, guild_id, channel.get()).await?;
    if removed {
        let card = generator_deleted_card(channel.get());
        ctx.send(card.reply()).await?;
    } else {
        let card = error_card("That channel is not registered as a Temporary VC generator.");
        ctx.send(card.reply()).await?;
    }
    Ok(())
}

/// List all registered generator channels in the server.
#[poise::command(slash_command, prefix_command, rename = "list")]
pub async fn admin_list(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();

    let generators = store::list_generators(&ctx.data().db, guild_id).await?;
    if generators.is_empty() {
        let card = Card::new(
            "📡 TempVC Generators",
            "No generator voice channels are configured on this server.",
        )
        .kind(CardKind::Info);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    let mut lines = Vec::new();
    for (i, g) in generators.iter().enumerate() {
        lines.push(format!(
            "**{}.** <#{}> — Name: `{}` | Limit: `{}`",
            i + 1,
            g.channel_id,
            g.name,
            if g.limit == 0 { "Unlimited".to_string() } else { g.limit.to_string() }
        ));
    }

    let card = Card::lines("📡 TempVC Generator Channels", lines).kind(CardKind::Info);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

pub fn commands() -> Vec<Command> {
    vec![tempvc()]
}
