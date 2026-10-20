use std::sync::atomic::{AtomicBool, Ordering};

use poise::serenity_prelude as serenity;

/// Global commands were stale once we moved to guild-only sync; clearing them
/// costs a REST call, so do it at most once per process boot, from one shard.
static GLOBAL_COMMANDS_CLEARED: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
pub struct Events;

#[serenity::async_trait]
impl serenity::EventHandler for Events {
    async fn dispatch(&self, ctx: &serenity::Context, event: &serenity::FullEvent) {
        match event {
            serenity::FullEvent::Ready { data_about_bot, .. } => {
                tracing::info!(user = %data_about_bot.user.name, "ready");
                // Slash commands are synced guild-only in `GuildCreate` (instant
                // propagation, no duplicates); drop any leftover global set.
                if !GLOBAL_COMMANDS_CLEARED.swap(true, Ordering::AcqRel) {
                    // PUT with an empty list deletes the global commands.
                    if let Err(error) = ctx
                        .http
                        .create_global_commands(&Vec::<serenity::CreateCommand>::new())
                        .await
                    {
                        GLOBAL_COMMANDS_CLEARED.store(false, Ordering::Release);
                        tracing::error!(%error, "failed to clear global slash commands");
                    }
                }
            }
            serenity::FullEvent::GuildCreate { guild, is_new, .. } => {
                tracing::debug!(guild_id = %guild.id, new = ?is_new, "guild create");
                let services = ctx.data::<lumi_kernel::bot::Data>();
                let commands = crate::commands::all_from_registry(&services.registry);
                let _ = poise::builtins::register_in_guild(&ctx.http, &commands, guild.id).await;
            }
            serenity::FullEvent::InteractionCreate { interaction, .. } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                match interaction {
                    serenity::Interaction::Component(component) => {
                        if let Err(error) =
                            crate::interactions::handle_component(ctx, services.as_ref(), component)
                                .await
                        {
                            tracing::warn!(%error, "component interaction failed");
                        }
                    }
                    serenity::Interaction::Modal(modal) => {
                        if let Err(error) =
                            crate::interactions::handle_modal(ctx, services.as_ref(), modal).await
                        {
                            tracing::warn!(%error, "modal interaction failed");
                        }
                    }
                    _ => {}
                }
            }
            serenity::FullEvent::Message { new_message, .. } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if lumi_modules::filter::events::on_message(ctx, services.as_ref(), new_message)
                    .await
                {
                    return;
                }

                if let Err(error) =
                    lumi_modules::afk::on_message(ctx, services.as_ref(), new_message).await
                {
                    tracing::warn!(%error, "afk message handler failed");
                }

                if let Err(error) =
                    lumi_modules::leveling::on_message(ctx, services.as_ref(), new_message).await
                {
                    tracing::warn!(%error, "leveling message handler failed");
                }
            }
            serenity::FullEvent::MessageDelete {
                channel_id,
                deleted_message_id,
                guild_id,
                ..
            } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if let Err(error) = lumi_modules::logging::events::on_message_delete(
                    ctx,
                    services.as_ref(),
                    *channel_id,
                    *deleted_message_id,
                    *guild_id,
                )
                .await
                {
                    tracing::warn!(%error, "logging message delete failed");
                }
            }
            serenity::FullEvent::MessageUpdate {
                old_if_available,
                event,
                ..
            } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if lumi_modules::filter::events::on_message_update(ctx, services.as_ref(), event)
                    .await
                {
                    return;
                }

                if let Err(error) = lumi_modules::logging::events::on_message_update(
                    ctx,
                    services.as_ref(),
                    old_if_available.clone(),
                    event.clone(),
                )
                .await
                {
                    tracing::warn!(%error, "logging message update failed");
                }
            }
            serenity::FullEvent::GuildMemberAddition { new_member, .. } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if let Err(error) =
                    lumi_modules::logging::events::on_member_add(ctx, services.as_ref(), new_member)
                        .await
                {
                    tracing::warn!(%error, "logging member add failed");
                }
            }
            serenity::FullEvent::GuildMemberRemoval { guild_id, user, .. } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if let Err(error) = lumi_modules::logging::events::on_member_remove(
                    ctx,
                    services.as_ref(),
                    *guild_id,
                    user,
                )
                .await
                {
                    tracing::warn!(%error, "logging member remove failed");
                }
            }
            serenity::FullEvent::GuildMemberUpdate {
                old_if_available,
                new,
                event,
                ..
            } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if let Err(error) = lumi_modules::logging::events::on_member_update(
                    ctx,
                    services.as_ref(),
                    old_if_available.clone(),
                    event.clone(),
                )
                .await
                {
                    tracing::warn!(%error, "logging member update failed");
                }
                if let Err(error) = lumi_modules::security::events::on_member_update(
                    ctx,
                    services.as_ref(),
                    old_if_available.as_ref(),
                    new.as_ref(),
                )
                .await
                {
                    tracing::warn!(%error, "security member update handler failed");
                }
            }
            serenity::FullEvent::GuildBanAddition {
                guild_id,
                banned_user,
                ..
            } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if let Err(error) = lumi_modules::logging::events::on_ban_add(
                    ctx,
                    services.as_ref(),
                    *guild_id,
                    banned_user,
                )
                .await
                {
                    tracing::warn!(%error, "logging ban add failed");
                }
            }
            serenity::FullEvent::GuildBanRemoval {
                guild_id,
                unbanned_user,
                ..
            } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if let Err(error) = lumi_modules::logging::events::on_ban_remove(
                    ctx,
                    services.as_ref(),
                    *guild_id,
                    unbanned_user,
                )
                .await
                {
                    tracing::warn!(%error, "logging ban remove failed");
                }
            }
            serenity::FullEvent::GuildUpdate {
                old_data_if_available,
                new_data,
                ..
            } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if let Err(error) = lumi_modules::security::events::on_guild_update(
                    ctx,
                    services.as_ref(),
                    old_data_if_available.as_ref(),
                    new_data,
                )
                .await
                {
                    tracing::warn!(%error, "security guild update handler failed");
                }
            }
            serenity::FullEvent::GuildRoleUpdate {
                old_data_if_available,
                new,
                ..
            } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if let Err(error) = lumi_modules::security::events::on_role_update(
                    ctx,
                    services.as_ref(),
                    old_data_if_available.as_ref(),
                    new,
                )
                .await
                {
                    tracing::warn!(%error, "security role update handler failed");
                }
            }
            serenity::FullEvent::GuildAuditLogEntryCreate {
                entry, guild_id, ..
            } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if let Err(error) = lumi_modules::security::events::on_audit_log_entry(
                    ctx,
                    services.as_ref(),
                    entry,
                    *guild_id,
                )
                .await
                {
                    tracing::warn!(%error, "security audit log handler failed");
                }
            }
            serenity::FullEvent::VoiceStateUpdate { old, new, .. } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if let Err(error) =
                    lumi_modules::tempvc::on_voice_state_update(ctx, services.as_ref(), old, new)
                        .await
                {
                    tracing::warn!(%error, "tempvc voice state update handler failed");
                }
                if let Err(error) =
                    lumi_modules::leveling::on_voice_state_update(ctx, services.as_ref(), old, new)
                        .await
                {
                    tracing::warn!(%error, "leveling voice state update handler failed");
                }
            }
            _ => {}
        }
    }
}
