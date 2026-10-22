//! Generic confirm/cancel prompt shared by destructive commands.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serenity::Error;
use serenity::all::{ChannelId, Context, CreateInteractionResponse, UserId};

use crate::cards::{Card, CardKind, CardOptions};
use crate::panels::confirm_row;

/// Send a confirmation card and wait up to `timeout` for `author_id` to press
/// Confirm or Cancel. Returns `false` on cancel, timeout or any send failure.
pub async fn confirm(
    ctx: &Context,
    channel_id: ChannelId,
    author_id: UserId,
    title: &str,
    body: &str,
    confirm_label: &str,
    timeout: Duration,
) -> Result<bool, Error> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let confirm_id = format!("confirm:{nonce}:yes");
    let cancel_id = format!("confirm:{nonce}:no");

    let options = CardOptions {
        action_rows: vec![confirm_row(
            confirm_id.clone(),
            cancel_id,
            Some(confirm_label),
            Some("Cancel"),
        )],
        ..CardOptions::default()
    };
    let card = Card::lines(title, vec![body.to_string()])
        .kind(CardKind::Warning)
        .options(options);

    let message = channel_id
        .widen()
        .send_message(&ctx.http, card.into_reply().message())
        .await?;

    let press = serenity::collector::ComponentInteractionCollector::new(ctx)
        .message_id(message.id)
        .author_id(author_id)
        .timeout(timeout)
        .await;

    let Some(press) = press else {
        drop(message.delete(&ctx.http, None).await);
        return Ok(false);
    };

    drop(
        press
            .create_response(&ctx.http, CreateInteractionResponse::Acknowledge)
            .await,
    );
    let confirmed = press.data.custom_id == confirm_id;
    drop(message.delete(&ctx.http, None).await);
    Ok(confirmed)
}

/// Send a confirmation prompt in a Poise command context, edit-clearing buttons on completion.
pub async fn confirm_poise<U: Send + Sync + 'static, E: 'static>(
    ctx: poise::Context<'_, U, E>,
    title: &str,
    body: &str,
    confirm_label: &str,
    timeout: Duration,
) -> Result<bool, Error> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    let confirm_id = format!("confirm:{nonce}:yes");
    let cancel_id = format!("confirm:{nonce}:no");

    let options = CardOptions {
        action_rows: vec![confirm_row(
            confirm_id.clone(),
            cancel_id,
            Some(confirm_label),
            Some("Cancel"),
        )],
        ..CardOptions::default()
    };
    let card = Card::lines(title, vec![body.to_string()])
        .kind(CardKind::Warning)
        .options(options);

    let Ok(reply) = ctx.send(card.into_reply().reply()).await else {
        return Ok(false);
    };
    let Ok(msg) = reply.message().await else {
        return Ok(false);
    };

    let press = serenity::collector::ComponentInteractionCollector::new(ctx.serenity_context())
        .message_id(msg.id)
        .author_id(ctx.author().id)
        .timeout(timeout)
        .await;

    let Some(press) = press else {
        drop(
            reply
                .edit(ctx, poise::CreateReply::default().components(Vec::new()))
                .await,
        );
        return Ok(false);
    };

    drop(
        press
            .create_response(ctx.http(), CreateInteractionResponse::Acknowledge)
            .await,
    );
    let confirmed = press.data.custom_id == confirm_id;
    drop(
        reply
            .edit(ctx, poise::CreateReply::default().components(Vec::new()))
            .await,
    );
    Ok(confirmed)
}
