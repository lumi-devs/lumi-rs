use async_trait::async_trait;
use poise::serenity_prelude as serenity;

#[derive(Debug)]
pub struct Events;

#[async_trait]
impl serenity::EventHandler for Events {
    async fn dispatch(&self, ctx: &serenity::Context, event: &serenity::FullEvent) {
        match event {
            serenity::FullEvent::Ready { data_about_bot, .. } => {
                tracing::info!(user = %data_about_bot.user.name, "ready");
            }
            serenity::FullEvent::GuildCreate { guild, is_new, .. } => {
                tracing::debug!(guild_id = %guild.id, new = ?is_new, "guild create");
            }
            serenity::FullEvent::InteractionCreate {
                interaction: serenity::Interaction::Component(component),
                ..
            } => {
                if let Err(error) =
                    crate::commands::handle_component(ctx, 0, component).await
                {
                    tracing::warn!(%error, "ping component interaction failed");
                }
            }
            serenity::FullEvent::Message { new_message, .. } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if !new_message.author.bot {
                    let guild_id = new_message.guild_id.map_or(0, serenity::GuildId::get);
                    let author_id = new_message.author.id.get();
                    let verdict = services.filter.check_content(
                        guild_id,
                        &new_message.content,
                        author_id,
                    );

                    if !verdict.is_clean() {
                        tracing::warn!(
                            ?verdict,
                            guild_id,
                            author_id,
                            message_id = %new_message.id,
                            "content filter violation detected, deleting message"
                        );
                        if let Err(error) = new_message.delete(&ctx.http).await {
                            tracing::warn!(%error, "failed to delete filtered message");
                        }
                        return;
                    }
                }

                if let Err(error) =
                    lumi_modules::afk::on_message(ctx, services.as_ref(), new_message).await
                {
                    tracing::warn!(%error, "afk message handler failed");
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
                if !event.author.as_ref().is_some_and(|a| a.bot) {
                    if let Some(content) = &event.content {
                        let guild_id = event.guild_id.map_or(0, serenity::GuildId::get);
                        let author_id = event
                            .author
                            .as_ref()
                            .map_or(0, |a| a.id.get());
                        let verdict = services.filter.check_content(
                            guild_id,
                            content,
                            author_id,
                        );

                        if !verdict.is_clean() {
                            tracing::warn!(
                                ?verdict,
                                guild_id,
                                author_id,
                                message_id = %event.id,
                                "content filter violation detected in updated message, deleting message"
                            );
                            if let Err(error) = ctx.http.delete_message(event.channel_id, event.id, Some("content filter violation")).await {
                                tracing::warn!(%error, "failed to delete filtered updated message");
                            }
                            return;
                        }
                    }
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
                    lumi_modules::logging::events::on_member_add(ctx, services.as_ref(), new_member).await
                {
                    tracing::warn!(%error, "logging member add failed");
                }
            }
            serenity::FullEvent::GuildMemberRemoval {
                guild_id, user, ..
            } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if let Err(error) = lumi_modules::logging::events::on_member_remove(
                    ctx, services.as_ref(), *guild_id, user,
                )
                .await
                {
                    tracing::warn!(%error, "logging member remove failed");
                }
            }
            serenity::FullEvent::GuildMemberUpdate {
                old_if_available,
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
            _ => {}
        }
    }
}
