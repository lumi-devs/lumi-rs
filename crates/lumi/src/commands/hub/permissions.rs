use poise::serenity_prelude as serenity;

use lumi_kernel::bot::Error;
use lumi_kernel::Services;
use lumi_ui::{Card, CardReply};

use super::{GuildCtx, HubId, HubOutcome};

/// The permits overview.
pub async fn list_card(
    _services: &Services,
    _guild: &GuildCtx,
    _page: usize,
    _user_id: u64,
) -> Result<CardReply, Error> {
    Ok(Card::info("🛡️ Permits", "No permits are assigned yet.").into_reply())
}

/// Route a permit-related interaction.
pub async fn dispatch(
    _services: &Services,
    _guild: &GuildCtx,
    _id: &HubId,
    _data: &serenity::ComponentInteractionData,
    _user_id: u64,
) -> Result<HubOutcome, Error> {
    Ok(HubOutcome::Done)
}
