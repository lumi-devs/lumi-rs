//! Shared confirmation prompt.
//!
//! Any module can gate a destructive action behind a Danger/Cancel button pair.
//! The collector is scoped to the prompt message and the invoking user, so
//! concurrent prompts never cross wires.
//!
//! Cross-agent note: this belongs in `lumi_ui` (or `lumi_kernel::bot`) next to
//! `panels::confirm_row` so UI callers share one implementation; it lives here
//! only because `crates/ui` is out of this worktree's scope.

use std::time::Duration;

use poise::serenity_prelude as serenity;

use lumi_kernel::bot::{Context, Error};
use lumi_ui::{Card, CardKind, CardOptions};

/// Show a confirm/cancel card and return `true` only on Confirm.
///
/// `namespace` namespaces the button ids (e.g. `"moderation"`); the collector
/// only listens to this message's buttons, so any stable value works.
pub async fn confirm(
    ctx: Context<'_>,
    namespace: &str,
    title: &str,
    body: &str,
    confirm_label: &str,
) -> Result<bool, Error> {
    let confirm_id = format!("{namespace}:confirm:{}", uuid::Uuid::new_v4());
    let cancel_id = format!("{namespace}:cancel:{}", uuid::Uuid::new_v4());

    let mut options = CardOptions::default();
    options.action_rows = vec![lumi_ui::panels::confirm_row(
        &confirm_id,
        &cancel_id,
        Some(confirm_label),
        Some("Cancel"),
    )];

    let card = Card::lines(title, vec![body.to_string()])
        .kind(CardKind::Warning)
        .options(options);

    let reply = ctx.send(card.into_reply().reply()).await?;
    let message = reply.message().await?;

    let interaction =
        serenity::collector::ComponentInteractionCollector::new(ctx.serenity_context())
            .message_id(message.id)
            .author_id(ctx.author().id)
            .timeout(Duration::from_secs(30))
            .await;

    let Some(press) = interaction else {
        let _ = reply
            .edit(ctx, poise::CreateReply::default().components(vec![]))
            .await;
        return Ok(false);
    };

    let _ = press
        .create_response(ctx.http(), serenity::CreateInteractionResponse::Acknowledge)
        .await;
    let _ = reply
        .edit(ctx, poise::CreateReply::default().components(vec![]))
        .await;

    Ok(press.data.custom_id == confirm_id)
}
