use poise::serenity_prelude as serenity;
use serenity::all::{
    ButtonStyle, ChannelId, ComponentInteraction, ComponentInteractionDataKind,
    CreateAllowedMentions, CreateButton, CreateInteractionResponse,
    CreateInteractionResponseMessage, GenericChannelId, GuildId, MessageFlags, PermissionOverwrite,
    PermissionOverwriteType, Permissions, UserId,
};

use lumi_kernel::Services;
use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::panels::{action_row_of, user_select};
use lumi_ui::{Card, CardKind, CardOptions, make_list_card};

use crate::shared::{require_guild, truncate_chars};

use super::store;
use super::views::{error_card, generator_deleted_card, generator_setup_card};

/// Discord's hard limit for voice channel user limits.
const MAX_USER_LIMIT: i32 = 99;
/// Discord's hard limit for channel names.
const MAX_CHANNEL_NAME: usize = 100;

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
    let guild_id = require_guild(ctx)?.get();
    let user_id = ctx.author().id.get();

    let Some(record) = store::get_active_channel(&ctx.data().db, guild_id, user_id).await? else {
        ctx.send(
            error_card(
                "You do not own an active temporary voice channel. Join a generator channel first.",
            )
            .reply(),
        )
        .await?;
        return Ok(());
    };

    let ch_id = record.channel_id.get();

    let row1 = action_row_of(vec![
        button("lock", ch_id, "Lock", '🔒', ButtonStyle::Secondary),
        button("unlock", ch_id, "Unlock", '🔓', ButtonStyle::Secondary),
        button("hide", ch_id, "Hide", '👁', ButtonStyle::Secondary),
        button("unhide", ch_id, "Unhide", '👀', ButtonStyle::Secondary),
    ]);
    let row2 = action_row_of(vec![
        button("trust", ch_id, "Trust", '🤝', ButtonStyle::Secondary),
        button("untrust", ch_id, "Untrust", '👋', ButtonStyle::Secondary),
        button("block", ch_id, "Block", '🚫', ButtonStyle::Secondary),
        button("kick", ch_id, "Kick", '👢', ButtonStyle::Secondary),
    ]);
    let row3 = action_row_of(vec![
        button(
            "transfer",
            ch_id,
            "Transfer Ownership",
            '👑',
            ButtonStyle::Primary,
        ),
        button("delete", ch_id, "Delete VC", '🗑', ButtonStyle::Danger),
    ]);

    let mut opts = CardOptions::default();
    opts.action_rows = vec![row1, row2, row3];

    let card = Card::lines(
        "🎙️ Voice Channel Control Deck",
        vec![
            format!("**Managed Channel**: <#{ch_id}>"),
            "Use the buttons below to manage access, lock, or transfer your channel.".to_string(),
        ],
    )
    .kind(CardKind::Primary)
    .options(opts);

    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

fn action_id(action: &str, ch_id: u64) -> String {
    format!("tempvc:{action}:{ch_id}")
}

fn button(
    action: &str,
    ch_id: u64,
    label: &str,
    emoji: char,
    style: ButtonStyle,
) -> CreateButton<'static> {
    CreateButton::new(action_id(action, ch_id))
        .label(label.to_string())
        .emoji(emoji)
        .style(style)
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
    subcommands(
        "admin_setup",
        "admin_delete",
        "admin_list",
        "admin_rename",
        "admin_limit"
    )
)]
pub async fn admin_cmd(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let card = Card::new(
        "🛠️ TempVC Admin Tools",
        "Use `/tempvc admin setup <channel>` to register a generator, `rename`/`limit` to change its defaults, or `list` to review them.",
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
    let guild_id = require_guild(ctx)?.get();
    let template_name = truncate_chars(
        name.as_deref().unwrap_or("{user}'s Channel").trim(),
        MAX_CHANNEL_NAME,
    );
    let user_limit = limit.unwrap_or(0).clamp(0, MAX_USER_LIMIT);

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

/// Change the name template of a generator channel.
#[poise::command(slash_command, prefix_command, rename = "rename")]
pub async fn admin_rename(
    ctx: Context<'_>,
    #[description = "The generator voice channel"] channel: GenericChannelId,
    #[description = "New template name (e.g. {user}'s Lounge)"] name: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?.get();
    let name = truncate_chars(name.trim(), MAX_CHANNEL_NAME);
    if name.is_empty() {
        ctx.send(error_card("The template name cannot be empty.").reply())
            .await?;
        return Ok(());
    }
    let updated =
        store::update_generator(&ctx.data().db, guild_id, channel.get(), Some(name.clone()), None)
            .await?;
    let card = if updated {
        Card::success(
            "Generator Renamed",
            format!("New template: `{name}`."),
        )
    } else {
        Card::error(
            "Generator Not Found",
            "That channel is not registered as a Temporary VC generator.",
        )
    };
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Change the default user limit of a generator channel.
#[poise::command(slash_command, prefix_command, rename = "limit")]
pub async fn admin_limit(
    ctx: Context<'_>,
    #[description = "The generator voice channel"] channel: GenericChannelId,
    #[description = "User limit (0 for unlimited)"] limit: i32,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?.get();
    let limit = limit.clamp(0, MAX_USER_LIMIT);
    let updated =
        store::update_generator(&ctx.data().db, guild_id, channel.get(), None, Some(limit)).await?;
    let card = if updated {
        Card::success(
            "Generator Limit Updated",
            if limit == 0 {
                "Generated channels will have no user limit.".to_string()
            } else {
                format!("Generated channels will be limited to **{limit}** members.")
            },
        )
    } else {
        Card::error(
            "Generator Not Found",
            "That channel is not registered as a Temporary VC generator.",
        )
    };
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Unregister a generator channel.
#[poise::command(slash_command, prefix_command, rename = "delete")]
pub async fn admin_delete(
    ctx: Context<'_>,
    #[description = "The generator voice channel to remove"] channel: GenericChannelId,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?.get();

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
    let guild_id = require_guild(ctx)?.get();

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

    let items: Vec<String> = generators
        .iter()
        .map(|g| {
            let limit = if g.limit == 0 {
                "Unlimited".to_string()
            } else {
                g.limit.to_string()
            };
            format!("<#{}> — `{}` (limit {limit})", g.channel_id, g.name)
        })
        .collect();

    ctx.send(make_list_card("📡 TempVC Generators", &items, CardOptions::default()).reply())
        .await?;
    Ok(())
}

fn is_admin(ctx: &serenity::Context, guild_id: GuildId, user_id: UserId) -> bool {
    ctx.cache.guild(guild_id).is_some_and(|guild| {
        guild.members.get(&user_id).is_some_and(|member| {
            let perms = guild.member_permissions(member);
            perms.contains(Permissions::ADMINISTRATOR) || perms.contains(Permissions::MANAGE_CHANNELS)
        })
    })
}

async fn respond_card(
    ctx: &serenity::Context,
    interaction: &ComponentInteraction,
    card: lumi_ui::CardReply,
    update: bool,
) -> Result<(), Error> {
    let mut message = CreateInteractionResponseMessage::new()
        .components(card.components)
        .allowed_mentions(CreateAllowedMentions::new())
        .flags(MessageFlags::IS_COMPONENTS_V2);
    let response = if update {
        CreateInteractionResponse::UpdateMessage(message)
    } else {
        message = message.flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL);
        CreateInteractionResponse::Message(message)
    };
    interaction.create_response(&ctx.http, response).await?;
    Ok(())
}

fn picker_card(action: &str, ch_id: u64) -> Card {
    let (title, body) = match action {
        "trust" => (
            "🤝 Trust a Member",
            "Pick a member who may connect to your channel.",
        ),
        "untrust" => (
            "👋 Untrust a Member",
            "Pick a member whose explicit access should be removed.",
        ),
        "block" => (
            "🚫 Block a Member",
            "Pick a member to deny access to your channel.",
        ),
        "kick" => (
            "👢 Kick a Member",
            "Pick a member to disconnect from your channel.",
        ),
        "transfer" => (
            "👑 Transfer Ownership",
            "Pick the member who should own this channel.",
        ),
        _ => ("TempVC", "Pick a member."),
    };
    let mut opts = CardOptions::default();
    opts.action_rows = vec![user_select(
        format!("tempvc:pick:{action}:{ch_id}"),
        "Select a member",
    )];
    Card::new(title, body)
        .kind(CardKind::Info)
        .options(opts)
        .ephemeral()
}

async fn apply_member_action(
    ctx: &serenity::Context,
    db: &sea_orm::DatabaseConnection,
    interaction: &ComponentInteraction,
    action: &str,
    guild_id: GuildId,
    ch_id: u64,
    target: UserId,
) -> Result<(), Error> {
    let channel = ChannelId::new(ch_id);
    let member = PermissionOverwriteType::Member(target);
    let result = match action {
        "trust" => {
            channel
                .create_permission(
                    &ctx.http,
                    PermissionOverwrite {
                        allow: Permissions::CONNECT | Permissions::VIEW_CHANNEL,
                        deny: Permissions::empty(),
                        kind: member,
                    },
                    Some("TempVC: Trust"),
                )
                .await
                .map_err(Error::from)
                .map(|()| format!("Trusted <@{target}>."))
        }
        "untrust" => channel
            .delete_permission(&ctx.http, member, Some("TempVC: Untrust"))
            .await
            .map_err(Error::from)
            .map(|()| format!("Removed <@{target}> from trusted members.")),
        "block" => channel
            .create_permission(
                &ctx.http,
                PermissionOverwrite {
                    allow: Permissions::empty(),
                    deny: Permissions::CONNECT | Permissions::VIEW_CHANNEL,
                    kind: member,
                },
                Some("TempVC: Block"),
            )
                .await
                .map_err(Error::from)
                .map(|()| format!("Blocked <@{target}>.")),
        "kick" => guild_id
            .disconnect_member(&ctx.http, target)
            .await
            .map_err(Error::from)
            .map(|_| format!("Disconnected <@{target}> from the channel.")),
        "transfer" => {
            store::set_owner(db, guild_id.get(), ch_id, target.get())
                .await
                .map_err(Error::from)
                .map(|_| format!("Ownership transferred to <@{target}>."))
        }
        _ => return Ok(()),
    };

    match result {
        Ok(message) => {
            let card = Card::success("TempVC Updated", message);
            respond_card(ctx, interaction, card.into_reply(), true).await?;
        }
        Err(error) => {
            tracing::warn!(%error, action, "tempvc member action failed");
            let card = Card::error(
                "TempVC Action Failed",
                "I could not apply that change. Check my permissions in this channel.",
            );
            respond_card(ctx, interaction, card.into_reply(), true).await?;
        }
    }
    Ok(())
}

fn selected_user(interaction: &ComponentInteraction) -> Option<UserId> {
    match &interaction.data.kind {
        ComponentInteractionDataKind::UserSelect { values } => values.first().copied(),
        _ => None,
    }
}

pub async fn handle_component(
    ctx: &serenity::Context,
    services: &Services,
    interaction: &ComponentInteraction,
) -> Result<(), Error> {
    let id = interaction.data.custom_id.as_str();
    let parts: Vec<&str> = id.split(':').collect();

    let (action, ch_id, target) = match parts.as_slice() {
        ["tempvc", action, ch] => {
            let Some(ch_id) = ch.parse::<u64>().ok() else {
                return Ok(());
            };
            (*action, ch_id, None)
        }
        ["tempvc", "pick", action, ch] => {
            let Some(ch_id) = ch.parse::<u64>().ok() else {
                return Ok(());
            };
            let Some(target) = selected_user(interaction) else {
                return Ok(());
            };
            (*action, ch_id, Some(target))
        }
        _ => return Ok(()),
    };

    let Some(guild_id) = interaction.guild_id else {
        return Ok(());
    };
    let Some(rec) = store::get_record(&services.db, guild_id.get(), ch_id).await? else {
        let card = error_card("This voice channel is no longer active or managed by TempVC.");
        respond_card(ctx, interaction, card, false).await?;
        return Ok(());
    };

    let caller_id = interaction.user.id;
    let owner_id = rec.owner_id.get();
    let is_owner = caller_id.get() == owner_id;

    if action == "claim" {
        let connected = ctx.cache.guild(guild_id).is_some_and(|guild| {
            guild
                .voice_states
                .get(&caller_id)
                .is_some_and(|state| state.channel_id == Some(ChannelId::new(ch_id)))
        });
        if !connected {
            let card = error_card(
                "You must be connected to this voice channel to claim ownership.",
            );
            respond_card(ctx, interaction, card, false).await?;
            return Ok(());
        }
        store::set_owner(&services.db, guild_id.get(), ch_id, caller_id.get()).await?;
        let card = Card::success("👑 Ownership Claimed", "You now own this channel.");
        respond_card(ctx, interaction, card.into_reply(), true).await?;
        return Ok(());
    }

    if !is_owner && !is_admin(ctx, guild_id, caller_id) {
        let card = error_card("Only the channel owner or staff can use these controls.");
        respond_card(ctx, interaction, card, false).await?;
        return Ok(());
    }

    // Buttons that need a target open an ephemeral user picker.
    if target.is_none() && matches!(action, "trust" | "untrust" | "block" | "kick" | "transfer") {
        let card = picker_card(action, ch_id);
        respond_card(ctx, interaction, card.into_reply(), true).await?;
        return Ok(());
    }

    if let Some(target) = target {
        return apply_member_action(
            ctx,
            &services.db,
            interaction,
            action,
            guild_id,
            ch_id,
            target,
        )
        .await;
    }

    let channel = ChannelId::new(ch_id);
    let everyone = PermissionOverwriteType::Role(guild_id.everyone_role());

    match action {
        "lock" | "unlock" => {
            let locked = action == "lock";
            store::set_locked(&services.db, guild_id.get(), ch_id, locked).await?;
            let overwrite = PermissionOverwrite {
                allow: Permissions::empty(),
                deny: Permissions::CONNECT,
                kind: everyone,
            };
            let result = if locked {
                channel
                    .create_permission(&ctx.http, overwrite, Some("TempVC: Lock"))
                    .await
            } else {
                channel
                    .delete_permission(&ctx.http, everyone, Some("TempVC: Unlock"))
                    .await
            };
            finish_privacy(ctx, interaction, result, locked).await?;
        }
        "hide" | "unhide" => {
            let hidden = action == "hide";
            store::set_hidden(&services.db, guild_id.get(), ch_id, hidden).await?;
            let overwrite = PermissionOverwrite {
                allow: Permissions::empty(),
                deny: Permissions::VIEW_CHANNEL,
                kind: everyone,
            };
            let result = if hidden {
                channel
                    .create_permission(&ctx.http, overwrite, Some("TempVC: Hide"))
                    .await
            } else {
                channel
                    .delete_permission(&ctx.http, everyone, Some("TempVC: Unhide"))
                    .await
            };
            finish_privacy(ctx, interaction, result, hidden).await?;
        }
        "delete" => {
            store::delete_record(&services.db, guild_id.get(), ch_id).await?;
            if let Err(error) = ctx
                .http
                .delete_channel(
                    GenericChannelId::new(ch_id),
                    Some("TempVC: Deleted by owner"),
                )
                .await
            {
                tracing::warn!(%error, channel_id = ch_id, "tempvc channel delete failed");
            }
            let card = Card::success("🗑 Channel Deleted", "Your temporary channel was removed.");
            respond_card(ctx, interaction, card.into_reply(), true).await?;
        }
        other => {
            tracing::debug!(action = other, "unhandled tempvc action");
        }
    }
    Ok(())
}

async fn finish_privacy(
    ctx: &serenity::Context,
    interaction: &ComponentInteraction,
    result: Result<(), serenity::Error>,
    enabled: bool,
) -> Result<(), Error> {
    match result {
        Ok(()) => {
            let card = if enabled {
                Card::success("Channel Privacy Updated", "The channel is now restricted.")
            } else {
                Card::success("Channel Privacy Updated", "The channel is now open again.")
            };
            respond_card(ctx, interaction, card.into_reply(), true).await?;
        }
        Err(error) => {
            tracing::warn!(%error, "tempvc permission update failed");
            let card = Card::error(
                "TempVC Action Failed",
                "I could not update the permission overwrite. Check my permissions.",
            );
            respond_card(ctx, interaction, card.into_reply(), false).await?;
        }
    }
    Ok(())
}

pub fn commands() -> Vec<Command> {
    vec![tempvc()]
}
