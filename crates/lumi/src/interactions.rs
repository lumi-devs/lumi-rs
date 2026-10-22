//! Custom-id registry and shared interaction router.

use poise::serenity_prelude as serenity;
use serenity::all::{ComponentInteraction, ModalInteraction};

use lumi_kernel::Services;
use lumi_kernel::bot::Error;

/// Hub interaction custom ID prefix.
pub const HUB_PREFIX: &str = crate::commands::hub::PREFIX;
/// `TempVC` interaction custom ID prefix.
pub const TEMPVC_PREFIX: &str = "tempvc";
/// Media interaction custom ID prefix.
pub const MEDIA_PREFIX: &str = "media";
/// Economy interaction custom ID prefix.
pub const ECONOMY_PREFIX: &str = "econ";
/// Ping interaction custom ID prefix.
pub const PING_PREFIX: &str = "ping";
/// AFK interaction custom ID prefix.
pub const AFK_PREFIX: &str = lumi_modules::afk::PREFIX;

/// Component surfaces routed by the gateway.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentRoute {
    /// Hub control panel.
    Hub,
    /// Temporary voice channel controls.
    TempVc,
    /// Media attachments.
    Media,
    /// Economy interactions.
    Economy,
    /// Ping dashboard interactions.
    Ping,
    /// AFK status interactions.
    Afk,
}

/// Prefix registry: the single source of truth for component routing.
pub const COMPONENT_ROUTES: &[(&str, ComponentRoute)] = &[
    (HUB_PREFIX, ComponentRoute::Hub),
    (TEMPVC_PREFIX, ComponentRoute::TempVc),
    (MEDIA_PREFIX, ComponentRoute::Media),
    (ECONOMY_PREFIX, ComponentRoute::Economy),
    (PING_PREFIX, ComponentRoute::Ping),
    (AFK_PREFIX, ComponentRoute::Afk),
];

impl ComponentRoute {
    /// Classify a custom id by prefix.
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
    /// Hub modal dialogs.
    Hub,
    /// `TempVC` rename and setup modal dialogs.
    TempVc,
}

impl ModalRoute {
    /// Classify a modal custom id by prefix.
    #[must_use]
    pub fn from_custom_id(custom_id: &str) -> Option<Self> {
        match custom_id.split(':').next()? {
            HUB_PREFIX => Some(Self::Hub),
            TEMPVC_PREFIX => Some(Self::TempVc),
            _ => None,
        }
    }
}

/// The one component router. Unknown prefixes are ignored.
#[tracing::instrument(
    name = "interaction.component",
    skip(ctx, services, component),
    fields(
        guild_id = ?component.guild_id.map(poise::serenity_prelude::GuildId::get),
        channel_id = %component.channel_id,
        user_id = %component.user.id,
        custom_id = %component.data.custom_id,
    )
)]
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
            crate::commands::general::handle_component(ctx, services, component).await
        }
        Some(ComponentRoute::Afk) => {
            lumi_modules::afk::handle_component(ctx, services, component).await
        }
        None => Ok(()),
    }
}

/// The one modal router.
#[tracing::instrument(
    name = "interaction.modal",
    skip(ctx, services, modal),
    fields(
        guild_id = ?modal.guild_id.map(poise::serenity_prelude::GuildId::get),
        channel_id = %modal.channel_id,
        user_id = %modal.user.id,
        custom_id = %modal.data.custom_id,
    )
)]
pub async fn handle_modal(
    ctx: &serenity::Context,
    services: &Services,
    modal: &ModalInteraction,
) -> Result<(), Error> {
    match ModalRoute::from_custom_id(&modal.data.custom_id) {
        Some(ModalRoute::Hub) => crate::commands::handle_hub_modal(ctx, services, modal).await,
        Some(ModalRoute::TempVc) => lumi_modules::tempvc::handle_modal(ctx, services, modal).await,
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_by_prefix_without_panicking_on_utf8() {
        assert_eq!(
            ComponentRoute::from_custom_id("lumi:home:1"),
            Some(ComponentRoute::Hub)
        );
        assert_eq!(
            ComponentRoute::from_custom_id("tempvc:lock:1"),
            Some(ComponentRoute::TempVc)
        );
        assert_eq!(
            ComponentRoute::from_custom_id("media:view:avatar:1"),
            Some(ComponentRoute::Media)
        );
        assert_eq!(
            ComponentRoute::from_custom_id("econ:lb:2"),
            Some(ComponentRoute::Economy)
        );
        assert_eq!(
            ComponentRoute::from_custom_id("ping:p_1:3"),
            Some(ComponentRoute::Ping)
        );
        assert_eq!(
            ComponentRoute::from_custom_id("afk:mentions:1:0"),
            Some(ComponentRoute::Afk)
        );
        assert_eq!(ComponentRoute::from_custom_id("nope:1"), None);
        assert_eq!(ComponentRoute::from_custom_id(""), None);
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
        assert_eq!(
            ModalRoute::from_custom_id("lumi:prefixmodal"),
            Some(ModalRoute::Hub)
        );
        assert_eq!(ModalRoute::from_custom_id("help:1"), None);
    }
}
