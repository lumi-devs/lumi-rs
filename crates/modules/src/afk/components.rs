//! The `afk:` component surface: the paged "who mentioned you" panel.

use chrono::Utc;
use poise::serenity_prelude as serenity;
use serenity::all::{
    ChannelId, ComponentInteraction, CreateAllowedMentions, CreateInteractionResponse,
    CreateInteractionResponseMessage, GenericChannelId, MessageFlags, MessageId,
};

use lumi_kernel::Services;
use lumi_kernel::bot::Error;

use crate::shared::format_seconds;

use super::cards::mentions_card;
use super::mentions::{self, Mention};

/// Custom-id prefix owned by this module.
pub const PREFIX: &str = "afk";

/// Mentions rendered per page.
const PAGE_SIZE: usize = 5;

/// Mention records kept at most; paging never exceeds this.
pub async fn handle_component(
    ctx: &serenity::Context,
    services: &Services,
    component: &ComponentInteraction,
) -> Result<(), Error> {
    let mut parts = component.data.custom_id.split(':');
    if parts.next() != Some(PREFIX) || parts.next() != Some("mentions") {
        return Ok(());
    }
    let (Some(user_id), Some(page)) = (
        parts.next().and_then(|value| value.parse::<u64>().ok()),
        parts.next().and_then(|value| value.parse::<usize>().ok()),
    ) else {
        return Ok(());
    };
    if component.user.id.get() != user_id {
        return Ok(());
    }
    let Some(guild_id) = component.guild_id else {
        return Ok(());
    };

    let rows = mentions::list(&services.cache, guild_id.get(), user_id).await?;
    let total_pages = rows.len().div_ceil(PAGE_SIZE).max(1);
    let page = page.min(total_pages - 1);
    let items: Vec<String> = rows
        .iter()
        .skip(page * PAGE_SIZE)
        .take(PAGE_SIZE)
        .map(|mention| render_mention(ctx, guild_id.get(), mention))
        .collect();
    let card = mentions_card(
        &items,
        page,
        total_pages,
        &format!("{PREFIX}:mentions:{user_id}"),
    );

    let ephemeral_source = component
        .message
        .flags
        .is_some_and(|flags| flags.contains(MessageFlags::EPHEMERAL));
    let response = if ephemeral_source {
        CreateInteractionResponse::UpdateMessage(
            CreateInteractionResponseMessage::new()
                .components(card.components)
                .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                .allowed_mentions(CreateAllowedMentions::new()),
        )
    } else {
        CreateInteractionResponse::Message(
            CreateInteractionResponseMessage::new()
                .components(card.components)
                .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                .allowed_mentions(CreateAllowedMentions::new()),
        )
    };
    component.create_response(&ctx.http, response).await?;
    Ok(())
}

/// `@user in #channel - 2h 1m ago` plus a jump link, rendered from cache only.
fn render_mention(ctx: &serenity::Context, guild_id: u64, mention: &Mention) -> String {
    let channel = ctx
        .cache
        .guild(serenity::GuildId::new(guild_id))
        .and_then(|guild| {
            guild
                .channels
                .get(&ChannelId::new(mention.channel_id))
                .map(|channel| channel.base.name.to_string())
        })
        .unwrap_or_else(|| "deleted channel".to_string());
    let age = format_seconds(
        (Utc::now().fixed_offset().timestamp() - mention.ts).max(0),
    );
    let link = MessageId::new(mention.message_id).link(
        GenericChannelId::new(mention.channel_id),
        Some(serenity::GuildId::new(guild_id)),
    );
    format!(
        "<@{}> in #{channel} - {age} ago\n[Jump to Message]({link})",
        mention.author_id
    )
}
