//! Custom-id registry and shared interaction router.
//!
//! Every component/modal prefix the host understands is declared once in
//! [`COMPONENT_ROUTES`]. Adding a new interactive surface means adding one route
//! there and one match arm in [`handle_component`] or [`handle_modal`]; the
//! gateway itself never inspects raw custom ids.

use poise::serenity_prelude as serenity;
use serenity::all::{
    ComponentInteraction, CreateAllowedMentions, CreateInteractionResponse,
    CreateInteractionResponseMessage, MessageFlags, MessageInteractionMetadata, ModalInteraction,
    UserId,
};

use lumi_kernel::bot::{Context, Error};
use lumi_kernel::Services;
use lumi_ui::{Card, CardKind, CardOptions, CardReply};

pub const HUB_PREFIX: &str = crate::commands::hub::PREFIX;
pub const TEMPVC_PREFIX: &str = "tempvc";
pub const MEDIA_PREFIX: &str = "media";
pub const ECONOMY_PREFIX: &str = "econ";
pub const PING_PREFIX: &str = lumi_ui::ping::PING_ID_PREFIX;

/// Component surfaces routed by the gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentRoute {
    Hub,
    TempVc,
    Media,
    Economy,
    Ping,
}

/// Prefix registry: the single source of truth for component routing.
pub const COMPONENT_ROUTES: &[(&str, ComponentRoute)] = &[
    (HUB_PREFIX, ComponentRoute::Hub),
    (TEMPVC_PREFIX, ComponentRoute::TempVc),
    (MEDIA_PREFIX, ComponentRoute::Media),
    (ECONOMY_PREFIX, ComponentRoute::Economy),
    (PING_PREFIX, ComponentRoute::Ping),
];

impl ComponentRoute {
    /// Classify a raw custom id by its prefix. Splits on `:` instead of slicing
    /// bytes, so arbitrary UTF-8 custom ids cannot panic.
    #[must_use]
    pub fn from_custom_id(custom_id: &str) -> Option<Self> {
        let prefix = custom_id.split(':').next()?;
        COMPONENT_ROUTES
            .iter()
            .find_map(|(candidate, route)| (*candidate == prefix).then_some(*route))
    }
}

/// Modal surfaces routed by the gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalRoute {
    Hub,
}

impl ModalRoute {
    #[must_use]
    pub fn from_custom_id(custom_id: &str) -> Option<Self> {
        match custom_id.split(':').next()? {
            HUB_PREFIX => Some(Self::Hub),
            _ => None,
        }
    }
}

/// The one component router. Unknown prefixes are ignored.
pub async fn handle_component(
    ctx: &serenity::Context,
    services: &Services,
    component: &ComponentInteraction,
) -> Result<(), Error> {
    match ComponentRoute::from_custom_id(&component.data.custom_id) {
        Some(ComponentRoute::Hub) => {
            crate::commands::handle_hub_component(ctx, services, component).await
        }
        Some(ComponentRoute::TempVc) => {
            lumi_modules::tempvc::handle_component(ctx, services, component).await
        }
        Some(ComponentRoute::Media) => {
            lumi_modules::utility::handle_component(ctx, services, component).await
        }
        Some(ComponentRoute::Economy) => {
            lumi_modules::economy::handle_component(ctx, services, component).await
        }
        Some(ComponentRoute::Ping) => {
            crate::commands::handle_component(ctx, 0, component).await
        }
        None => Ok(()),
    }
}

/// The one modal router.
pub async fn handle_modal(
    ctx: &serenity::Context,
    services: &Services,
    modal: &ModalInteraction,
) -> Result<(), Error> {
    match ModalRoute::from_custom_id(&modal.data.custom_id) {
        Some(ModalRoute::Hub) => crate::commands::handle_hub_modal(ctx, services, modal).await,
        None => Ok(()),
    }
}

/// The user who originally rendered a component panel.
///
/// Discord attaches this to interaction response messages, so panels can be
/// owner-locked without trusting a user id embedded in the custom id (which is
/// rewritten by some UI builders).
#[must_use]
pub fn panel_owner(component: &ComponentInteraction) -> Option<UserId> {
    let metadata = component.message.interaction_metadata.as_deref()?;
    match metadata {
        MessageInteractionMetadata::Command(meta) => Some(meta.user.id),
        MessageInteractionMetadata::Component(meta) => Some(meta.user.id),
        MessageInteractionMetadata::ModalSubmit(meta) => Some(meta.user.id),
        _ => None,
    }
}

/// Whether `component.user` owns the panel. Falls back to the id encoded in the
/// custom id when Discord did not attach interaction metadata.
#[must_use]
pub fn caller_is_owner(component: &ComponentInteraction, fallback_id: u64) -> bool {
    panel_owner(component).map_or_else(
        || fallback_id == 0 || fallback_id == component.user.id.get(),
        |owner| owner == component.user.id,
    )
}

/// Replace the panel with a freshly rendered card.
pub async fn update_message(
    http: &serenity::Http,
    component: &ComponentInteraction,
    reply: &CardReply,
) -> Result<(), serenity::Error> {
    component
        .create_response(
            http,
            CreateInteractionResponse::UpdateMessage(
                CreateInteractionResponseMessage::new()
                    .components(reply.components.clone())
                    .allowed_mentions(CreateAllowedMentions::new())
                    .flags(MessageFlags::IS_COMPONENTS_V2),
            ),
        )
        .await
}

/// Send an ephemeral (only-visible-to-caller) text response.
pub async fn reply_ephemeral(
    http: &serenity::Http,
    component: &ComponentInteraction,
    content: impl Into<String>,
) -> Result<(), serenity::Error> {
    let card = Card::warning("Notice", content.into()).ephemeral();
    let reply = card.into_reply();
    component
        .create_response(
            http,
            CreateInteractionResponse::Message(
                CreateInteractionResponseMessage::new()
                    .components(reply.components)
                    .flags(MessageFlags::IS_COMPONENTS_V2 | MessageFlags::EPHEMERAL)
                    .allowed_mentions(CreateAllowedMentions::new()),
            ),
        )
        .await
}

/// Generic confirmation collector: prompts the command caller with a warning
/// card and Confirm/Cancel buttons, awaiting their decision for up to 30s.
pub async fn confirm(
    ctx: Context<'_>,
    title: &str,
    body: &str,
    confirm_label: Option<&str>,
) -> Result<bool, Error> {
    let confirm_id = "confirm:yes";
    let cancel_id = "confirm:no";
    let opts = CardOptions {
        action_rows: vec![lumi_ui::panels::confirm_row(
            confirm_id,
            cancel_id,
            confirm_label,
            Some("Cancel"),
        )],
        ..Default::default()
    };

    let card = Card::lines(title, vec![body.to_string()])
        .kind(CardKind::Warning)
        .options(opts);

    let reply = ctx.send(card.into_reply().reply()).await?;
    let msg = reply.message().await?;

    let interaction = serenity::collector::ComponentInteractionCollector::new(ctx.serenity_context())
        .message_id(msg.id)
        .author_id(ctx.author().id)
        .timeout(std::time::Duration::from_secs(30))
        .await;

    let Some(press) = interaction else {
        let _ = reply
            .edit(ctx, poise::CreateReply::default().components(Vec::new()))
            .await;
        return Ok(false);
    };

    let _ = press
        .create_response(
            ctx.http(),
            CreateInteractionResponse::Acknowledge,
        )
        .await;

    let _ = reply
        .edit(ctx, poise::CreateReply::default().components(Vec::new()))
        .await;

    Ok(press.data.custom_id == confirm_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_by_prefix_without_panicking_on_utf8() {
        assert_eq!(ComponentRoute::from_custom_id("lumi:home:1"), Some(ComponentRoute::Hub));
        assert_eq!(ComponentRoute::from_custom_id("tempvc:lock:1"), Some(ComponentRoute::TempVc));
        assert_eq!(ComponentRoute::from_custom_id("media:view:avatar:1"), Some(ComponentRoute::Media));
        assert_eq!(ComponentRoute::from_custom_id("econ:lb:2"), Some(ComponentRoute::Economy));
        assert_eq!(ComponentRoute::from_custom_id("ping:overview:3"), Some(ComponentRoute::Ping));
        assert_eq!(ComponentRoute::from_custom_id("nope:1"), None);
        assert_eq!(ComponentRoute::from_custom_id(""), None);
        // Multi-byte prefix must not hit a byte slice.
        assert_eq!(ComponentRoute::from_custom_id("h\u{e9}llo:1"), None);
    }

    #[test]
    fn every_registered_prefix_has_a_route() {
        for (prefix, expected) in COMPONENT_ROUTES {
            let id = format!("{prefix}:x");
            assert_eq!(ComponentRoute::from_custom_id(&id), Some(*expected));
        }
    }

    #[test]
    fn modal_routes() {
        assert_eq!(ModalRoute::from_custom_id("lumi:prefixmodal"), Some(ModalRoute::Hub));
        assert_eq!(ModalRoute::from_custom_id("help:1"), None);
    }
}
