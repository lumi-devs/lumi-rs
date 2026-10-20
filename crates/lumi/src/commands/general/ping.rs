use poise::serenity_prelude as serenity;
use serenity::all::{
    CacheHttp, ComponentInteraction, CreateAllowedMentions, CreateInteractionResponse,
    CreateInteractionResponseMessage, HttpError, JsonErrorCode, MessageFlags,
};

use lumi_kernel::Services;
use lumi_kernel::bot::{Context, Error};
use lumi_ui::ping::{build_overview_card, shard_page_count};

use crate::interactions::PING_PREFIX;
use crate::ping::collect;

/// Health dashboard — per-shard status, latency and system health.
#[poise::command(slash_command, prefix_command, aliases("pong", "latency"))]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let data = collect(ctx.data().as_ref(), ctx.serenity_context()).await;
    let user_id = ctx.author().id.get();
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

/// Whether an error means the source message or interaction token is gone.
fn is_gone(error: &serenity::Error) -> bool {
    matches!(
        error,
        serenity::Error::Http(HttpError::UnsuccessfulRequest(response))
            if matches!(
                response.error.code,
                JsonErrorCode::UnknownMessage | JsonErrorCode::UnknownInteraction
            )
    )
}

/// Dashboard buttons: `ping:p:<page>:<user>`, `ping:n:<page>:<user>` and
/// `ping:close:<user>`. The page lives in the id, so nothing is stored.
pub async fn handle_component(
    ctx: &serenity::Context,
    services: &Services,
    component: &ComponentInteraction,
) -> Result<(), Error> {
    let mut parts = component.data.custom_id.split(':').skip(1);
    let Some(action) = parts.next() else {
        return Ok(());
    };

    let (requested_page, user_id) = if action == "close" {
        let Some(user_id) = parts.next().and_then(|id| id.parse::<u64>().ok()) else {
            return Ok(());
        };
        (None, user_id)
    } else {
        let Some(page) = parts.next().and_then(|page| page.parse::<usize>().ok()) else {
            return Ok(());
        };
        let Some(user_id) = parts.next().and_then(|id| id.parse::<u64>().ok()) else {
            return Ok(());
        };
        (Some(page), user_id)
    };

    if component.user.id.get() != user_id {
        return Ok(());
    }

    if action == "close" {
        if let Err(error) = component
            .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
            .await
        {
            if is_gone(&error) {
                tracing::debug!(%error, user_id, "ping dashboard already closed");
                return Ok(());
            }
            return Err(error.into());
        }
        if let Err(error) = ctx
            .http
            .delete_original_interaction_response(&component.token)
            .await
        {
            if is_gone(&error) {
                tracing::debug!(%error, user_id, "ping dashboard message is gone");
                return Ok(());
            }
            return Err(error.into());
        }
        return Ok(());
    }

    let Some(requested_page) = requested_page else {
        return Ok(());
    };

    if action != "p" && action != "n" {
        return Ok(());
    }

    let data = collect(services, ctx).await;
    let page = requested_page.min(shard_page_count(&data).saturating_sub(1));
    let card = build_overview_card(&data, user_id, page, PING_PREFIX);

    let mut flags = MessageFlags::IS_COMPONENTS_V2;
    if component
        .message
        .flags
        .is_some_and(|flags| flags.contains(MessageFlags::EPHEMERAL))
    {
        flags |= MessageFlags::EPHEMERAL;
    }

    if let Err(error) = component
        .create_response(
            ctx.http(),
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .components(card.components)
                    .flags(flags)
                    .allowed_mentions(CreateAllowedMentions::new()),
            ),
        )
        .await
    {
        if is_gone(&error) {
            tracing::debug!(%error, user_id, "ping dashboard message is gone");
            if let Err(ack) = component
                .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
                .await
            {
                tracing::debug!(error = %ack, user_id, "ping dashboard could not acknowledge");
            }
            return Ok(());
        }
        return Err(error.into());
    }

    Ok(())
}
