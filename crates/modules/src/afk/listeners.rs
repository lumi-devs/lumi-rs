use poise::serenity_prelude as serenity;

use super::cards::{mention_card, welcome_back_card};
use super::data::{clear, get};
use lumi_kernel::Result;

/// Message listener: clear the author's AFK, then notify about mentioned AFK users.
pub async fn on_message(
    ctx: &serenity::Context,
    services: &lumi_kernel::Services,
    message: &serenity::Message,
) -> Result<()> {
    if message.author.bot() {
        return Ok(());
    }
    let Some(guild_id) = message.guild_id else {
        return Ok(());
    };
    let guild = guild_id.get();

    if clear(&services.db, guild, message.author.id.get()).await? {
        let card = welcome_back_card(&message.author.name);
        message.channel_id.send_message(&ctx.http, card.message()).await?;
    }

    for user in &message.mentions {
        if user.bot() {
            continue;
        }
        if let Some(entry) = get(&services.db, guild, user.id.get()).await? {
            let card = mention_card(
                &user.name,
                &entry.reason,
                &entry.since.to_rfc3339(),
                Some(user.face()),
            );
            message
                .channel_id
                .send_message(&ctx.http, card.message())
                .await?;
        }
    }

    Ok(())
}
