use poise::serenity_prelude as serenity;
use serenity::all::{
    CacheHttp, ComponentInteraction, CreateAllowedMentions, CreateInteractionResponse,
    CreateInteractionResponseMessage, MessageFlags,
};

use lumi_kernel::Services;
use lumi_kernel::bot::{Context, Error};
use lumi_ui::ping::{build_overview_card, shard_page_count};

use crate::interactions::PING_PREFIX;
use crate::ping::{collect, forget_page, move_page, reset_page, spawn_live_updates};

/// Health dashboard — per-shard status, latency and system health.
#[poise::command(slash_command, prefix_command, aliases("pong", "latency"))]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let data = collect(ctx.data().as_ref(), ctx.serenity_context()).await;
    let user_id = ctx.author().id.get();
    reset_page(user_id);
    let card = build_overview_card(&data, user_id, 0, PING_PREFIX);

    match ctx {
        poise::Context::Application(app) => {
            app.interaction
                .create_response(
                    ctx.http(),
                    CreateInteractionResponse::Message(
                        CreateInteractionResponseMessage::new()
                            .components(card.components)
                            .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                            .allowed_mentions(CreateAllowedMentions::new()),
                    ),
                )
                .await?;

            spawn_live_updates(
                std::sync::Arc::clone(ctx.data().as_ref()),
                ctx.serenity_context().clone(),
                app.interaction.token.to_string(),
                user_id,
            );
        }
        poise::Context::Prefix(pfx) => {
            pfx.msg
                .channel_id
                .send_message(
                    ctx.http(),
                    serenity::CreateMessage::new()
                        .reference_message(pfx.msg)
                        .components(card.components)
                        .flags(MessageFlags::IS_COMPONENTS_V2)
                        .allowed_mentions(CreateAllowedMentions::new()),
                )
                .await?;
        }
    }

    Ok(())
}

/// Pager and Close buttons on a rendered dashboard: `ping:p_<delta>:<user>` and
/// `ping:close:<user>`.
pub async fn handle_component(
    ctx: &serenity::Context,
    services: &Services,
    component: &ComponentInteraction,
) -> Result<(), Error> {
    let mut parts = component.data.custom_id.split(':').skip(1);
    let (Some(action), Some(user_id)) = (parts.next(), parts.next().and_then(|id| id.parse().ok()))
    else {
        return Ok(());
    };

    if component.user.id.get() != user_id {
        return Ok(());
    }

    if action == "close" {
        component
            .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
            .await?;
        forget_page(user_id);
        ctx.http
            .delete_original_interaction_response(&component.token)
            .await?;
        return Ok(());
    }

    let Some(delta) = action
        .strip_prefix("p_")
        .and_then(|d| d.parse::<i64>().ok())
    else {
        return Ok(());
    };

    let data = collect(services, ctx).await;
    let page = move_page(user_id, delta, shard_page_count(&data));
    let card = build_overview_card(&data, user_id, page, PING_PREFIX);

    let mut flags = MessageFlags::IS_COMPONENTS_V2;
    if component
        .message
        .flags
        .is_some_and(|flags| flags.contains(MessageFlags::EPHEMERAL))
    {
        flags |= MessageFlags::EPHEMERAL;
    }
    component
        .create_response(
            ctx.http(),
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .components(card.components)
                    .flags(flags)
                    .allowed_mentions(CreateAllowedMentions::new()),
            ),
        )
        .await?;

    Ok(())
}
