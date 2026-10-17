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
                let services = ctx.data::<crate::client::Data>();
                if let Err(error) =
                    crate::commands::handle_component(ctx, &services, component).await
                {
                    tracing::warn!(%error, "ping component interaction failed");
                }
            }
            serenity::FullEvent::Message { new_message, .. } => {
                let services = ctx.data::<crate::client::Data>();
                if let Err(error) =
                    lumi::modules::afk::on_message(ctx, &services, new_message).await
                {
                    tracing::warn!(%error, "afk message handler failed");
                }
            }
            _ => {}
        }
    }
}
