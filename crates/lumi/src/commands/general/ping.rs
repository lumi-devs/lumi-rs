use lumi_ui::ping::{
    OVERVIEW, PING_ID_PREFIX, PingCategory, build_detail_card, build_overview_card_paged,
};
use poise::serenity_prelude as serenity;
use serenity::{
    CreateAllowedMentions, CreateInteractionResponse, CreateInteractionResponseMessage,
    MessageFlags,
};

use lumi_kernel::bot::{Context, Error};
use crate::interactions::{caller_is_owner, reply_ephemeral, update_message};
use crate::ping::collect;

enum PingView {
    Overview { page: usize },
    Close,
    Category(PingCategory),
}

/// Health dashboard — bot status, latency and system health.
#[poise::command(slash_command, prefix_command)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let user_id = ctx.author().id.get();
    let data = collect(0, ctx.serenity_context()).await;
    let card = build_overview_card_paged(&data, 0, user_id);

    match ctx {
        poise::Context::Application(app) => {
            app.interaction
                .create_response(
                    ctx.http(),
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .components(card.components)
                            .flags(MessageFlags::IS_COMPONENTS_V2)
                            .allowed_mentions(CreateAllowedMentions::new()),
                    ),
                )
                .await?;
        }
        poise::Context::Prefix(pfx) => {
            pfx.msg
                .channel_id
                .send_message(
                    ctx.http(),
                    serenity::CreateMessage::new()
                        .components(card.components)
                        .flags(MessageFlags::IS_COMPONENTS_V2)
                        .allowed_mentions(CreateAllowedMentions::new()),
                )
                .await?;
        }
    }

    Ok(())
}

/// Handle a `ping:{category|p_±1|close}:{user_id}` button press.
pub async fn handle_component(
    ctx: &serenity::Context,
    addon_count: usize,
    component: &serenity::ComponentInteraction,
) -> Result<(), Error> {
    let Some((view, user_id)) = parse_view(&component.data.custom_id) else {
        return Ok(());
    };
    if !caller_is_owner(component, user_id) {
        reply_ephemeral(&ctx.http, component, "This panel belongs to someone else.").await?;
        return Ok(());
    }

    if matches!(view, PingView::Close) {
        component
            .create_response(&ctx.http, CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .content("*Closed.*")
                    .components(vec![])
                    .flags(MessageFlags::IS_COMPONENTS_V2),
            ))
            .await?;
        return Ok(());
    }

    let data = collect(addon_count, ctx).await;
    let card = match view {
        PingView::Overview { page } => build_overview_card_paged(&data, page, user_id),
        PingView::Category(category) => build_detail_card(category, &data, user_id),
        PingView::Close => return Ok(()),
    };

    update_message(&ctx.http, component, &card).await?;

    Ok(())
}

fn parse_view(custom_id: &str) -> Option<(PingView, u64)> {
    let mut parts = custom_id.split(':');
    if parts.next()? != PING_ID_PREFIX {
        return None;
    }
    let action = parts.next()?;
    let user_id: u64 = parts.next()?.parse().ok()?;

    let view = match action {
        "close" => PingView::Close,
        OVERVIEW => PingView::Overview { page: 0 },
        action => match action.strip_prefix("p_") {
            Some(delta) => {
                let delta: i64 = delta.parse().ok()?;
                let page = parts.next().and_then(|p| p.parse::<i64>().ok()).unwrap_or(0);
                let new_page = page.saturating_add(delta).max(0);
                PingView::Overview {
                    page: usize::try_from(new_page).unwrap_or(usize::MAX),
                }
            }
            None => PingView::Category(PingCategory::from_id(action)?),
        },
    };
    Some((view, user_id))
}
