use poise::serenity_prelude as serenity;
use serenity::all::{
    ButtonStyle, ChannelId, CreateActionRow, CreateButton, CreateComponent,
    CreateInteractionResponse, CreateInteractionResponseMessage, MessageFlags,
    PermissionOverwrite, PermissionOverwriteType, Permissions,
};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::{Card, CardKind, CardOptions};

use super::store;
use super::views::{error_card, generator_deleted_card, generator_setup_card};

/// Dynamic Temporary Voice Channel management.
#[poise::command(
    slash_command,
    guild_only,
    subcommands("panel", "admin")
)]
pub async fn tempvc(ctx: Context<'_>) -> Result<(), Error> {
    panel(ctx).await
}

/// Open your interactive TempVC channel control deck (ephemeral buttons).
#[poise::command(slash_command, guild_only)]
pub async fn panel(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let user_id = ctx.author().id.get();

    let record_opt = store::get_user_active_vc(&ctx.data().db, guild_id, user_id).await?;
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

    // Build interactive action rows matching TS TempVC panel
    let row1 = CreateActionRow::Buttons(vec![
        CreateButton::new(format!("tempvc:lock:{ch_id}"))
            .label("Lock Channel")
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
    ]);

    let row2 = CreateActionRow::Buttons(vec![
        CreateButton::new(format!("tempvc:claim:{ch_id}"))
            .label("Claim Ownership")
            .style(ButtonStyle::Primary)
            .emoji('👑'),
        CreateButton::new(format!("tempvc:delete:{ch_id}"))
            .label("Delete VC")
            .style(ButtonStyle::Danger)
            .emoji('🗑'),
    ]);

    let mut opts = CardOptions::default();
    opts.action_rows = vec![row1, row2];

    let card = Card::lines(title, lines)
        .kind(CardKind::Primary)
        .options(opts);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Administrative configuration for Temporary Voice Channel generators (Staff only).
#[poise::command(
    slash_command,
    guild_only,
    default_member_permissions = "MANAGE_CHANNELS",
    subcommands("admin_setup", "admin_delete", "admin_list")
)]
pub async fn admin(ctx: Context<'_>) -> Result<(), Error> {
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
#[poise::command(slash_command, rename = "setup")]
pub async fn admin_setup(
    ctx: Context<'_>,
    #[description = "The voice channel to act as generator"] channel: ChannelId,
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

/// Unregister a generator channel.
#[poise::command(slash_command, rename = "delete")]
pub async fn admin_delete(
    ctx: Context<'_>,
    #[description = "The generator voice channel to remove"] channel: ChannelId,
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
#[poise::command(slash_command, rename = "list")]
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
