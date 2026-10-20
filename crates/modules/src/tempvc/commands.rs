use poise::serenity_prelude as serenity;
use serenity::all::{
    ButtonStyle, ChannelId, ChannelType, ComponentInteraction, ComponentInteractionDataKind,
    CreateAllowedMentions, CreateButton, CreateInputText, CreateInteractionResponse,
    CreateInteractionResponseMessage, CreateLabel, CreateModal, CreateModalComponent, EditChannel,
    GuildId, InputTextStyle, MessageFlags, ModalInteraction, PermissionOverwrite,
    PermissionOverwriteType, Permissions, RoleId, UserId,
};

use lumi_kernel::Services;
use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::panels::action_row_of;
use lumi_ui::{Card, CardOptions, CardReply};

use crate::shared::{require_guild, truncate_chars};

use super::store;
use super::views::{self, GeneratorView};

/// Discord's hard limit for voice channel user limits.
const MAX_USER_LIMIT: i32 = 99;
/// Discord's hard limit for channel names.
const MAX_CHANNEL_NAME: usize = 100;

/// TempVC entry point. `,tempvc` (prefix) shows the panel directly.
#[poise::command(slash_command, prefix_command, guild_only, subcommands("panel_cmd", "admin_cmd"))]
pub async fn tempvc(ctx: Context<'_>) -> Result<(), Error> {
    run_panel(ctx).await
}

/// Open your temp VC control panel.
#[poise::command(slash_command, prefix_command, rename = "panel", guild_only)]
pub async fn panel_cmd(ctx: Context<'_>) -> Result<(), Error> {
    run_panel(ctx).await
}

async fn run_panel(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let guild_id = require_guild(ctx)?.get();
    let user_id = ctx.author().id.get();

    let Some(record) = store::get_active_channel(&ctx.data().db, guild_id, user_id).await? else {
        ctx.send(
            error_card("You do not own an active temporary voice channel. Join a generator channel first.")
                .reply(),
        )
        .await?;
        return Ok(());
    };

    let channel_id = record.channel_id.get();
    let card = views::panel_card(
        channel_id,
        record.owner_id.get(),
        record.locked,
        record.hidden,
        channel_limit(ctx.serenity_context(), guild_id, channel_id),
    );
    ctx.send(card.reply()).await?;
    Ok(())
}

/// Manage voice generator channels (staff only).
#[poise::command(
    slash_command,
    prefix_command,
    rename = "admin",
    guild_only,
    default_member_permissions = "MANAGE_CHANNELS"
)]
pub async fn admin_cmd(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;
    let guild_id = require_guild(ctx)?.get();
    let card = admin_card(ctx.serenity_context(), ctx.data().as_ref(), guild_id).await?;
    ctx.send(card.reply()).await?;
    Ok(())
}

async fn admin_card(
    ctx: &serenity::Context,
    services: &Services,
    guild_id: u64,
) -> Result<CardReply, Error> {
    let generators = generator_views(ctx, services, guild_id).await?;
    Ok(views::admin_card(&generators))
}

/// Resolve generator rows into views, preferring live channel names from the cache.
async fn generator_views(
    ctx: &serenity::Context,
    services: &Services,
    guild_id: u64,
) -> Result<Vec<GeneratorView>, Error> {
    let rows = store::list_generators(&services.db, guild_id).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            let channel_id = row.channel_id.get();
            let channel_name = ctx
                .cache
                .guild(GuildId::new(guild_id))
                .and_then(|guild| guild.channels.get(&ChannelId::new(channel_id)).cloned())
                .map_or_else(
                    || format!("\u{1f50a} {channel_id}"),
                    |channel| truncate_chars(&channel.base.name, MAX_CHANNEL_NAME),
                );
            GeneratorView {
                channel_id,
                channel_name,
                template: row.name,
                limit: row.limit,
            }
        })
        .collect())
}

fn is_admin(ctx: &serenity::Context, guild_id: GuildId, user_id: UserId) -> bool {
    ctx.cache.guild(guild_id).is_some_and(|guild| {
        guild.members.get(&user_id).is_some_and(|member| {
            let perms = guild.member_permissions(member);
            perms.contains(Permissions::ADMINISTRATOR)
                || perms.contains(Permissions::MANAGE_CHANNELS)
        })
    })
}

fn may_manage(ctx: &serenity::Context, services: &Services, guild_id: GuildId, user: UserId) -> bool {
    is_admin(ctx, guild_id, user) || services.cfg.is_owner(user.get())
}

fn error_card(message: &str) -> CardReply {
    Card::error("TempVC Error", message).into_reply()
}

/// Members currently connected to a voice channel, from the cache.
fn channel_members(ctx: &serenity::Context, guild_id: GuildId, channel: ChannelId) -> Vec<UserId> {
    ctx.cache
        .guild(guild_id)
        .and_then(|guild| guild.channels.get(&channel).cloned())
        .and_then(|c| c.members(&ctx.cache).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|member| member.user.id)
        .collect()
}

/// The live user limit of a voice channel (0 = unlimited).
fn channel_limit(ctx: &serenity::Context, guild_id: u64, channel_id: u64) -> i32 {
    ctx.cache
        .guild(GuildId::new(guild_id))
        .and_then(|guild| guild.channels.get(&ChannelId::new(channel_id)).cloned())
        .and_then(|channel| channel.user_limit)
        .map_or(0, |limit| i32::from(limit.get()))
}

async fn respond(
    ctx: &serenity::Context,
    interaction: &ComponentInteraction,
    card: CardReply,
    update: bool,
) -> Result<(), Error> {
    let mut message = CreateInteractionResponseMessage::new()
        .components(card.components)
        .allowed_mentions(CreateAllowedMentions::new())
        .flags(MessageFlags::IS_COMPONENTS_V2);
    let response = if update {
        CreateInteractionResponse::UpdateMessage(message)
    } else {
        message = message.flags(MessageFlags::EPHEMERAL);
        CreateInteractionResponse::Message(message)
    };
    interaction.create_response(&ctx.http, response).await?;
    Ok(())
}

/// Discord cannot update a message in response to a modal submit; this posts an
/// ephemeral result card (optionally with a "back to panel" row) instead.
async fn respond_modal(
    ctx: &serenity::Context,
    modal: &ModalInteraction,
    card: CardReply,
) -> Result<(), Error> {
    let message = CreateInteractionResponseMessage::new()
        .components(card.components)
        .allowed_mentions(CreateAllowedMentions::new())
        .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL);
    modal
        .create_response(&ctx.http, CreateInteractionResponse::Message(message))
        .await?;
    Ok(())
}

fn input_modal(
    custom_id: String,
    title: &str,
    label: &str,
    value: &str,
    max: usize,
) -> CreateModal<'static> {
    let mut input = CreateInputText::new(InputTextStyle::Short, "value")
        .required(true)
        .max_length(u16::try_from(max).unwrap_or(u16::MAX));
    if !value.is_empty() {
        input = input.value(value.to_string());
    }
    CreateModal::new(custom_id, title.to_string()).components(vec![
        CreateModalComponent::Label(CreateLabel::input_text(label.to_string(), input)),
    ])
}

async fn show_modal(
    ctx: &serenity::Context,
    interaction: &ComponentInteraction,
    modal: CreateModal<'static>,
) -> Result<(), Error> {
    interaction
        .create_response(&ctx.http, CreateInteractionResponse::Modal(modal))
        .await?;
    Ok(())
}

fn modal_input(modal: &ModalInteraction) -> String {
    for component in &modal.data.components {
        if let serenity::ModalComponent::Label(label) = component
            && let serenity::LabelComponent::InputText(input) = &label.component
        {
            return input.value.trim().to_string();
        }
    }
    String::new()
}

fn select_value(interaction: &ComponentInteraction) -> Option<String> {
    match &interaction.data.kind {
        ComponentInteractionDataKind::StringSelect { values } => values.first().cloned(),
        _ => None,
    }
}

fn selected_users(interaction: &ComponentInteraction) -> Vec<UserId> {
    match &interaction.data.kind {
        ComponentInteractionDataKind::UserSelect { values } => values.iter().copied().collect(),
        _ => Vec::new(),
    }
}

fn selected_roles(interaction: &ComponentInteraction) -> Vec<RoleId> {
    match &interaction.data.kind {
        ComponentInteractionDataKind::RoleSelect { values } => values.iter().copied().collect(),
        _ => Vec::new(),
    }
}

fn selected_channels(interaction: &ComponentInteraction) -> Vec<ChannelId> {
    match &interaction.data.kind {
        ComponentInteractionDataKind::ChannelSelect { values } => values.iter().copied().collect(),
        _ => Vec::new(),
    }
}

/// Re-render the channel panel from a fresh DB record and live channel limit.
async fn refresh_panel(
    ctx: &serenity::Context,
    services: &Services,
    guild_id: u64,
    channel_id: u64,
) -> Result<CardReply, Error> {
    let reply = store::get_record(&services.db, guild_id, channel_id)
        .await?
        .map_or_else(
            || views::error_card("This voice channel is no longer active."),
            |record| {
                views::panel_card(
                    record.channel_id.get(),
                    record.owner_id.get(),
                    record.locked,
                    record.hidden,
                    channel_limit(ctx, guild_id, channel_id),
                )
            },
        );
    Ok(reply)
}

/// The panel's own actions, keyed by the sequence text picked from its menu.
async fn panel_action(
    ctx: &serenity::Context,
    services: &Services,
    interaction: &ComponentInteraction,
    guild_id: GuildId,
    channel_id: u64,
    action: &str,
) -> Result<(), Error> {
    let Some(record) = store::get_record(&services.db, guild_id.get(), channel_id).await? else {
        return respond(
            ctx,
            interaction,
            error_card("This voice channel is no longer active or managed by TempVC."),
            false,
        )
        .await;
    };

    let caller = interaction.user.id;
    if action == "claim" {
        let connected = ctx.cache.guild(guild_id).is_some_and(|guild| {
            guild
                .voice_states
                .get(&caller)
                .is_some_and(|state| state.channel_id == Some(ChannelId::new(channel_id)))
        });
        if !connected {
            return respond(
                ctx,
                interaction,
                error_card("You must be connected to this voice channel to claim ownership."),
                true,
            )
            .await;
        }
        store::set_owner(&services.db, guild_id.get(), channel_id, caller.get()).await?;
        return respond(
            ctx,
            interaction,
            Card::success(
                "\u{1f451} Ownership Claimed",
                "You now own this channel and its controls.",
            )
            .into_reply(),
            true,
        )
        .await;
    }

    if caller.get() != record.owner_id.get() && !may_manage(ctx, services, guild_id, caller) {
        return respond(
            ctx,
            interaction,
            error_card("Only the channel owner or staff can use these controls."),
            true,
        )
        .await;
    }

    let channel = ChannelId::new(channel_id);
    let everyone = PermissionOverwriteType::Role(guild_id.everyone_role());

    match action {
        "back" => {
            let card = refresh_panel(ctx, services, guild_id.get(), channel_id).await?;
            respond(ctx, interaction, card, true).await
        }
        "name" => {
            let current = ctx
                .cache
                .guild(guild_id)
                .and_then(|guild| guild.channels.get(&channel).cloned())
                .map_or(String::new(), |c| c.base.name.into_string());
            show_modal(
                ctx,
                interaction,
                input_modal(
                    views::rename_modal_id(channel_id),
                    "Rename Voice Channel",
                    "New channel name",
                    &current,
                    MAX_CHANNEL_NAME,
                ),
            )
            .await
        }
        "limit" => {
            show_modal(
                ctx,
                interaction,
                input_modal(
                    views::limit_modal_id(channel_id),
                    "Set User Limit",
                    "User limit (0-99, 0 = unlimited)",
                    &channel_limit(ctx, guild_id.get(), channel_id).to_string(),
                    2,
                ),
            )
            .await
        }
        "lock" | "unlock" | "hide" | "unhide" => {
            let locking = action.starts_with("lock");
            let enabling = matches!(action, "lock" | "hide");
            let overwrite = PermissionOverwrite {
                allow: Permissions::empty(),
                deny: if locking {
                    Permissions::CONNECT
                } else {
                    Permissions::VIEW_CHANNEL
                },
                kind: everyone,
            };
            let gating = if enabling {
                channel
                    .create_permission(&ctx.http, overwrite, Some("TempVC"))
                    .await
            } else {
                channel
                    .delete_permission(&ctx.http, everyone, Some("TempVC"))
                    .await
            };
            if let Err(error) = gating {
                tracing::warn!(%error, guild_id = guild_id.get(), channel_id, action, "tempvc permission toggle failed");
                return respond(
                    ctx,
                    interaction,
                    error_card("I could not update the channel permissions. Check my permissions."),
                    true,
                )
                .await;
            }
            if enabling {
                let grant = if locking {
                    Permissions::CONNECT
                } else {
                    Permissions::VIEW_CHANNEL
                };
                for member in channel_members(ctx, guild_id, channel) {
                    if let Err(error) = channel
                        .create_permission(
                            &ctx.http,
                            PermissionOverwrite {
                                allow: grant,
                                deny: Permissions::empty(),
                                kind: PermissionOverwriteType::Member(member),
                            },
                            Some("TempVC keep access"),
                        )
                        .await
                    {
                        tracing::warn!(%error, channel_id, member = member.get(), "tempvc member overwrite failed");
                    }
                }
            }
            if locking {
                store::set_locked(&services.db, guild_id.get(), channel_id, enabling).await?;
            } else {
                store::set_hidden(&services.db, guild_id.get(), channel_id, enabling).await?;
            }
            let card = refresh_panel(ctx, services, guild_id.get(), channel_id).await?;
            respond(ctx, interaction, card, true).await
        }
        "kick" | "trust" | "untrust" | "block" | "unblock" | "transfer" => {
            respond(ctx, interaction, views::picker_card(action, channel_id), true).await
        }
        "delete" => {
            respond(ctx, interaction, views::delete_confirm_card(channel_id), true).await
        }
        "delok" => {
            store::delete_record(&services.db, guild_id.get(), channel_id).await?;
            if let Err(error) = ctx
                .http
                .delete_channel(channel.into(), Some("TempVC: deleted"))
                .await
            {
                tracing::warn!(%error, channel_id, "tempvc channel delete failed");
            }
            respond(
                ctx,
                interaction,
                Card::success(
                    "\u{1f5d1}\u{fe0f} Channel Deleted",
                    "Your temporary channel was removed.",
                )
                .into_reply(),
                true,
            )
            .await
        }
        other => {
            tracing::debug!(action = other, "unhandled tempvc panel action");
            Ok(())
        }
    }
}

fn back_button(channel_id: u64) -> serenity::all::CreateButton<'static> {
    CreateButton::new(views::back_id(channel_id))
        .label("\u{2190} Back to Panel")
        .style(ButtonStyle::Secondary)
}

fn with_back(card: Card, channel_id: u64) -> CardReply {
    let opts = CardOptions {
        separator_above_action_rows: true,
        action_rows: vec![action_row_of(vec![back_button(channel_id)])],
        ..CardOptions::default()
    };
    card.options(opts).into_reply()
}

/// Add or re-format a generator channel through the admin card.
async fn handle_admin(
    ctx: &serenity::Context,
    services: &Services,
    interaction: &ComponentInteraction,
    guild_id: GuildId,
    action: &str,
) -> Result<(), Error> {
    let parts: Vec<&str> = interaction.data.custom_id.split(':').collect();
    match action {
        "add" => {
            let Some(picked) = selected_channels(interaction).first().copied() else {
                return Ok(());
            };
            let is_voice = ctx
                .cache
                .guild(guild_id)
                .and_then(|guild| guild.channels.get(&picked).cloned())
                .is_some_and(|channel| channel.base.kind == ChannelType::Voice);
            if !is_voice {
                return respond(
                    ctx,
                    interaction,
                    error_card("Pick a voice channel: it becomes the generator."),
                    true,
                )
                .await;
            }
            store::create_generator(
                &services.db,
                guild_id.get(),
                picked.get(),
                "{user}'s Channel".to_string(),
                0,
            )
            .await?;
            let card = admin_card(ctx, services, guild_id.get()).await?;
            respond(ctx, interaction, card, true).await
        }
        "manage" => {
            let Some(picked) = select_value(interaction).and_then(|v| v.parse::<u64>().ok()) else {
                return Ok(());
            };
            let generators = generator_views(ctx, services, guild_id.get()).await?;
            let Some(generator) = generators.into_iter().find(|g| g.channel_id == picked) else {
                return respond(
                    ctx,
                    interaction,
                    error_card("That generator no longer exists."),
                    true,
                )
                .await;
            };
            respond(ctx, interaction, views::generator_card(&generator), true).await
        }
        "back" => {
            let card = admin_card(ctx, services, guild_id.get()).await?;
            respond(ctx, interaction, card, true).await
        }
        "rm" => {
            let Some(channel_id) = parts.get(3).and_then(|id| id.parse::<u64>().ok()) else {
                return Ok(());
            };
            store::delete_generator(&services.db, guild_id.get(), channel_id).await?;
            let card = admin_card(ctx, services, guild_id.get()).await?;
            respond(ctx, interaction, card, true).await
        }
        "name" | "limit" => {
            let Some(channel_id) = parts.get(3).and_then(|id| id.parse::<u64>().ok()) else {
                return Ok(());
            };
            let generators = generator_views(ctx, services, guild_id.get()).await?;
            let Some(generator) = generators.iter().find(|g| g.channel_id == channel_id) else {
                return Ok(());
            };
            let modal = if action == "name" {
                input_modal(
                    views::admin_name_modal_id(channel_id),
                    "Generator Name Template",
                    "Template ({user}, {number}, {pos})",
                    &generator.template,
                    MAX_CHANNEL_NAME,
                )
            } else {
                input_modal(
                    views::admin_limit_modal_id(channel_id),
                    "Generator User Limit",
                    "User limit (0-99, 0 = unlimited)",
                    &generator.limit.to_string(),
                    2,
                )
            };
            show_modal(ctx, interaction, modal).await
        }
        other => {
            tracing::debug!(action = other, "unhandled tempvc admin action");
            Ok(())
        }
    }
}

pub async fn handle_component(
    ctx: &serenity::Context,
    services: &Services,
    interaction: &ComponentInteraction,
) -> Result<(), Error> {
    let parts: Vec<&str> = interaction.data.custom_id.split(':').collect();
    let Some(guild_id) = interaction.guild_id else {
        return Ok(());
    };

    if parts.get(1) == Some(&"admin") {
        if !may_manage(ctx, services, guild_id, interaction.user.id) {
            return respond(
                ctx,
                interaction,
                error_card("Only staff can manage TempVC generators."),
                true,
            )
            .await;
        }
        return handle_admin(
            ctx,
            services,
            interaction,
            guild_id,
            parts.get(2).copied().unwrap_or_default(),
        )
        .await;
    }

    match parts.as_slice() {
        ["tempvc", "menu", channel] => {
            let Some(channel_id) = channel.parse::<u64>().ok() else {
                return Ok(());
            };
            let Some(picked) = select_value(interaction) else {
                return Ok(());
            };
            panel_action(ctx, services, interaction, guild_id, channel_id, &picked).await
        }
        ["tempvc", "claim" | "back" | "delok", channel] => {
            let Some(channel_id) = channel.parse::<u64>().ok() else {
                return Ok(());
            };
            panel_action(
                ctx,
                services,
                interaction,
                guild_id,
                channel_id,
                parts[1],
            )
            .await
        }
        ["tempvc", "pick" | "pickrole", action, channel] => {
            let Some(channel_id) = channel.parse::<u64>().ok() else {
                return Ok(());
            };
            handle_pick(ctx, services, interaction, guild_id, channel_id, action).await
        }
        _ => Ok(()),
    }
}

async fn handle_pick(
    ctx: &serenity::Context,
    services: &Services,
    interaction: &ComponentInteraction,
    guild_id: GuildId,
    channel_id: u64,
    action: &str,
) -> Result<(), Error> {
    let users = selected_users(interaction);
    let roles = selected_roles(interaction);
    if users.is_empty() && roles.is_empty() {
        return Ok(());
    }
    if action == "transfer" {
        let Some(target) = users.first() else {
            return Ok(());
        };
        store::set_owner(&services.db, guild_id.get(), channel_id, target.get()).await?;
        return respond(
            ctx,
            interaction,
            with_back(
                Card::success(
                    "\u{1f451} Ownership Transferred",
                    format!("<@{target}> now owns this channel."),
                ),
                channel_id,
            ),
            true,
        )
        .await;
    }
    if action == "kick" {
        let mut kicked = 0usize;
        for user in &users {
            if guild_id.disconnect_member(&ctx.http, *user).await.is_ok() {
                kicked += 1;
            }
        }
        return respond(
            ctx,
            interaction,
            with_back(
                Card::success(
                    "\u{1f462} Members Kicked",
                    format!("Disconnected **{kicked}** member(s) from the channel."),
                ),
                channel_id,
            ),
            true,
        )
        .await;
    }
    apply_access(ctx, interaction, action, channel_id, &users, &roles).await
}

async fn apply_access(
    ctx: &serenity::Context,
    interaction: &ComponentInteraction,
    action: &str,
    channel_id: u64,
    users: &[UserId],
    roles: &[RoleId],
) -> Result<(), Error> {
    let channel = ChannelId::new(channel_id);
    let allow = Permissions::CONNECT | Permissions::VIEW_CHANNEL | Permissions::SPEAK;
    let deny = if action == "block" {
        Permissions::CONNECT | Permissions::VIEW_CHANNEL
    } else {
        Permissions::empty()
    };
    let removing = matches!(action, "untrust" | "unblock");
    let mut done = 0usize;

    for kind in users
        .iter()
        .map(|id| PermissionOverwriteType::Member(*id))
        .chain(roles.iter().map(|id| PermissionOverwriteType::Role(*id)))
    {
        let result = if removing {
            channel
                .delete_permission(&ctx.http, kind, Some("TempVC"))
                .await
        } else {
            channel
                .create_permission(
                    &ctx.http,
                    PermissionOverwrite { allow, deny, kind },
                    Some("TempVC"),
                )
                .await
        };
        match result {
            Ok(()) => done += 1,
            Err(error) => tracing::warn!(%error, ?kind, action, "tempvc overwrite failed"),
        }
    }

    let card = if done == 0 {
        error_card("I could not apply that change. Check my permissions.")
    } else {
        Card::success(
            "TempVC Updated",
            format!("Applied **{action}** to **{done}** target(s)."),
        )
        .into_reply()
    };
    respond(ctx, interaction, card, true).await
}

pub async fn handle_modal(
    ctx: &serenity::Context,
    services: &Services,
    modal: &ModalInteraction,
) -> Result<(), Error> {
    let parts: Vec<&str> = modal.data.custom_id.split(':').collect();
    let Some(guild_id) = modal.guild_id else {
        return Ok(());
    };
    let Some(channel_id) = parts.last().and_then(|id| id.parse::<u64>().ok()) else {
        return Ok(());
    };
    let value = modal_input(modal);

    match parts.as_slice() {
        ["tempvc", "namem", _] => {
            let name = truncate_chars(value.trim(), MAX_CHANNEL_NAME);
            if name.is_empty() {
                return respond_modal(ctx, modal, error_card("Give the channel a name.")).await;
            }
            ChannelId::new(channel_id)
                .edit(&ctx.http, EditChannel::new().name(name.clone()))
                .await?;
            respond_modal(
                ctx,
                modal,
                with_back(
                    Card::success("Channel Renamed", format!("Now named **{name}**.")),
                    channel_id,
                ),
            )
            .await
        }
        ["tempvc", "limitm", _] => {
            let Ok(limit) = value.parse::<i32>() else {
                return respond_modal(ctx, modal, error_card("Enter 0-99 (0 = unlimited).")).await;
            };
            let limit = limit.clamp(0, MAX_USER_LIMIT);
            if let Some(limit) = u16::try_from(limit)
                .ok()
                .and_then(|limited| serenity::nonmax::NonMaxU16::try_from(limited).ok())
            {
                ChannelId::new(channel_id)
                    .edit(&ctx.http, EditChannel::new().user_limit(limit))
                    .await?;
            }
            respond_modal(
                ctx,
                modal,
                with_back(
                    Card::success(
                        "User Limit Updated",
                        if limit == 0 {
                            "The channel is unlimited again.".to_string()
                        } else {
                            format!("Limited to **{limit}** members.")
                        },
                    ),
                    channel_id,
                ),
            )
            .await
        }
        ["tempvc", "admin", "namem", _] => {
            if !may_manage(ctx, services, guild_id, modal.user.id) {
                return respond_modal(ctx, modal, error_card("Only staff can manage generators.")).await;
            }
            let name = truncate_chars(value.trim(), MAX_CHANNEL_NAME);
            if name.is_empty() {
                return respond_modal(ctx, modal, error_card("Give the template some text.")).await;
            }
            store::update_generator(&services.db, guild_id.get(), channel_id, Some(name.clone()), None)
                .await?;
            respond_generator(ctx, services, modal, guild_id.get(), channel_id).await
        }
        ["tempvc", "admin", "limitm", _] => {
            if !may_manage(ctx, services, guild_id, modal.user.id) {
                return respond_modal(ctx, modal, error_card("Only staff can manage generators.")).await;
            }
            let Ok(limit) = value.parse::<i32>() else {
                return respond_modal(
                    ctx,
                    modal,
                    error_card("Enter 0-99 (0 = unlimited)."),
                )
                .await;
            };
            let limit = limit.clamp(0, MAX_USER_LIMIT);
            store::update_generator(&services.db, guild_id.get(), channel_id, None, Some(limit)).await?;
            respond_generator(ctx, services, modal, guild_id.get(), channel_id).await
        }
        _ => Ok(()),
    }
}

async fn respond_generator(
    ctx: &serenity::Context,
    services: &Services,
    modal: &ModalInteraction,
    guild_id: u64,
    channel_id: u64,
) -> Result<(), Error> {
    let generators = generator_views(ctx, services, guild_id).await?;
    let reply = generators
        .iter()
        .find(|g| g.channel_id == channel_id)
        .map_or_else(
            || error_card("That generator no longer exists."),
            |generator| views::generator_card(generator),
        );
    respond_modal(ctx, modal, reply).await
}

pub fn commands() -> Vec<Command> {
    vec![tempvc()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_limit_clamps_to_discord_max() {
        assert_eq!(MAX_USER_LIMIT, 99);
        for raw in [-5, 0, 12, 500] {
            let clamped = raw.clamp(0, MAX_USER_LIMIT);
            assert!((0..=99).contains(&clamped));
        }
    }

    #[test]
    fn tempvc_has_panel_and_admin_subcommands() {
        let cmd = tempvc();
        assert_eq!(cmd.name, "tempvc");
        assert_eq!(cmd.subcommands.len(), 2);
        let names: Vec<String> = cmd
            .subcommands
            .iter()
            .map(|s| s.name.to_string())
            .collect();
        assert_eq!(names, ["panel", "admin"]);
        let admin = &cmd.subcommands[1];
        assert_eq!(
            admin.default_member_permissions,
            serenity::Permissions::MANAGE_CHANNELS
        );
    }

    #[test]
    fn modal_and_view_id_format_roundtrip() {
        assert_eq!(views::rename_modal_id(42), "tempvc:namem:42");
        assert_eq!(views::admin_remove_id(42), "tempvc:admin:rm:42");
        assert_eq!(views::admin_name_modal_id(42), "tempvc:admin:namem:42");
        assert_eq!(views::admin_limit_modal_id(42), "tempvc:admin:limitm:42");
        assert_eq!(views::menu_id(42), "tempvc:menu:42");
    }
}
