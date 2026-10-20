use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};
use std::sync::atomic::{AtomicBool, Ordering};

use poise::serenity_prelude as serenity;

pub mod dispatcher;
pub use dispatcher::{DispatchedEvent, EventDispatcher};

static DISPATCHER: OnceLock<EventDispatcher> = OnceLock::new();

fn get_or_init_dispatcher(
    ctx: &serenity::Context,
    services: &lumi_kernel::bot::Data,
) -> &'static EventDispatcher {
    DISPATCHER.get_or_init(|| {
        EventDispatcher::new(
            ctx.clone(),
            Arc::clone(services),
        )
    })
}

/// Global commands were stale once we moved to guild-only sync; clearing them
/// costs a REST call, so do it at most once per process boot, from one shard.
static GLOBAL_COMMANDS_CLEARED: AtomicBool = AtomicBool::new(false);

/// Hash of the command set this process registers, computed on first use.
static COMMAND_SET_HASH: OnceLock<Option<u64>> = OnceLock::new();

fn command_set_hash(services: &lumi_kernel::Services) -> Option<u64> {
    *COMMAND_SET_HASH.get_or_init(|| {
        let commands = crate::commands::all_from_registry(&services.registry);
        let builders = poise::builtins::create_application_commands(&commands);
        let serialized = match serde_json::to_string(&builders) {
            Ok(serialized) => serialized,
            Err(error) => {
                tracing::warn!(
                    %error,
                    "failed to serialize application commands; guild sync will not be cached"
                );
                return None;
            }
        };
        let mut hasher = DefaultHasher::new();
        serialized.hash(&mut hasher);
        Some(hasher.finish())
    })
}

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
                let services = ctx.data::<lumi_kernel::bot::Data>();
                let owns_shard_zero = services
                    .cfg
                    .shard_range
                    .map_or(true, |range| range.start == 0);
                if owns_shard_zero && !GLOBAL_COMMANDS_CLEARED.swap(true, Ordering::AcqRel) {
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
                let hash = command_set_hash(&services);
                let key = format!("lumi:cmd:guild:{}", guild.id);

                if let Some(hash) = hash {
                    match services.cache.get::<u64>(&key).await {
                        Ok(Some(cached)) if cached == hash => return,
                        Ok(_) => {}
                        Err(error) => {
                            tracing::warn!(
                                %error,
                                guild_id = %guild.id,
                                "failed to read guild command hash; registering"
                            );
                        }
                    }
                }

                if let Err(error) =
                    poise::builtins::register_in_guild(&ctx.http, &commands, guild.id).await
                {
                    tracing::warn!(%error, guild_id = %guild.id, "failed to register guild commands");
                    return;
                }

                if let Some(hash) = hash {
                    if let Err(error) = services.cache.set(&key, &hash).await {
                        tracing::warn!(
                            %error,
                            guild_id = %guild.id,
                            "failed to store guild command hash"
                        );
                    }
                }
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
                let dispatcher = get_or_init_dispatcher(ctx, services.as_ref());
                dispatcher.dispatch(DispatchedEvent::Message(Box::new(new_message.clone())));
            }
            serenity::FullEvent::MessageDelete {
                channel_id,
                deleted_message_id,
                guild_id,
                ..
            } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                let dispatcher = get_or_init_dispatcher(ctx, services.as_ref());
                dispatcher.dispatch(DispatchedEvent::MessageDelete {
                    channel_id: *channel_id,
                    message_id: *deleted_message_id,
                    guild_id: *guild_id,
                });
            }
            serenity::FullEvent::MessageUpdate {
                event,
                ..
            } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                let dispatcher = get_or_init_dispatcher(ctx, services.as_ref());
                dispatcher.dispatch(DispatchedEvent::MessageUpdate {
                    event: Box::new(event.clone()),
                });
            }
            serenity::FullEvent::GuildMemberAddition { new_member, .. } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                let dispatcher = get_or_init_dispatcher(ctx, services.as_ref());
                dispatcher.dispatch(DispatchedEvent::MemberAddition {
                    member: Box::new(new_member.clone()),
                });
            }
            serenity::FullEvent::GuildMemberRemoval {
                guild_id,
                user,
                member_data_if_available,
                ..
            } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                let dispatcher = get_or_init_dispatcher(ctx, services.as_ref());
                dispatcher.dispatch(DispatchedEvent::MemberRemoval {
                    guild_id: *guild_id,
                    user: user.clone(),
                    member_data_if_available: member_data_if_available
                        .as_ref()
                        .map(|m| Box::new(m.clone())),
                });
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
                let dispatcher = get_or_init_dispatcher(ctx, services.as_ref());
                dispatcher.dispatch(DispatchedEvent::AuditLogEntry {
                    entry: Box::new(entry.clone()),
                    guild_id: *guild_id,
                });
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
