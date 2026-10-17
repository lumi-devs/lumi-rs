use async_trait::async_trait;
use poise::serenity_prelude as serenity;

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
                if let Err(error) =
                    lumi_modules::afk::on_message(ctx, services, new_message).await
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
                    services,
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
                if let Err(error) = lumi_modules::logging::events::on_message_update(
                    ctx,
                    services,
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
                    lumi_modules::logging::events::on_member_add(ctx, services, new_member).await
                {
                    tracing::warn!(%error, "logging member add failed");
                }
            }
            serenity::FullEvent::GuildMemberRemoval {
                guild_id, user, ..
            } => {
                let services = ctx.data::<lumi_kernel::bot::Data>();
                if let Err(error) = lumi_modules::logging::events::on_member_remove(
                    ctx, services, *guild_id, user,
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
                    services,
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
                    services,
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
                    services,
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
