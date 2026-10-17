use lumi::ui::ping::{
    OVERVIEW, PING_ID_PREFIX, PingCategory, build_detail_card, build_overview_card,
};
use poise::serenity_prelude as serenity;
use serenity::{
    CreateAllowedMentions, CreateInteractionResponse, CreateInteractionResponseMessage,
    EditInteractionResponse, MessageFlags,
};

use crate::client::{Context, Error};

enum PingView {
    Overview,
    Category(PingCategory),
}

/// Health dashboard — bot status, latency and system health.
#[poise::command(slash_command)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer_ephemeral().await?;

    let cached = ctx.data();
    let data = lumi::ping::collect(&cached, ctx.serenity_context()).await;
    let card = build_overview_card(&data);

    let poise::Context::Application(app) = ctx else {
        return Ok(());
    };
    app.interaction
        .edit_response(
            ctx.http(),
            EditInteractionResponse::new()
                .components(card.components)
                .flags(MessageFlags::IS_COMPONENTS_V2)
                .allowed_mentions(CreateAllowedMentions::new()),
        )
        .await?;

    Ok(())
}

/// Handle a `ping:{category}:{user_id}` button press from the dashboard owner.
pub async fn handle_component(
    ctx: &serenity::Context,
    services: &lumi::Services,
    component: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let Some((view, user_id)) = parse_view(&component.data.custom_id) else {
        return Ok(());
    };
    if component.user.id.get() != user_id {
        return Ok(());
    }

    let data = lumi::ping::collect(services, ctx).await;
    let card = match view {
        PingView::Overview => build_overview_card(&data),
        PingView::Category(category) => build_detail_card(category, &data, user_id),
    };

    component
        .create_response(
            &ctx.http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .components(card.components)
                    .flags(MessageFlags::IS_COMPONENTS_V2)
                    .allowed_mentions(CreateAllowedMentions::new()),
            ),
        )
        .await?;

    Ok(())
}

fn parse_view(custom_id: &str) -> Option<(PingView, u64)> {
    let mut parts = custom_id.split(':');
    if parts.next()? != PING_ID_PREFIX {
        return None;
    }
    let category = parts.next()?;
    let user_id: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let view = if category == OVERVIEW {
        PingView::Overview
    } else {
        PingView::Category(PingCategory::from_id(category)?)
    };
    Some((view, user_id))
}
