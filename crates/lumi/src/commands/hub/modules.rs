use poise::serenity_prelude as serenity;

use lumi_kernel::bot::Error;
use lumi_kernel::Services;
use lumi_ui::{Card, CardKind, CardOptions, CardReply};

use super::{tab_row, GuildCtx, HubId, HubOutcome, HubView};

/// The feature/module list.
pub async fn list_card(
    services: &Services,
    guild: &GuildCtx,
    _page: usize,
    _user_id: u64,
) -> Result<CardReply, Error> {
    let body: Vec<String> = services
        .registry
        .iter()
        .map(|module| {
            let meta = module.metadata();
            format!(
                "{} **{}** `v{}`\n-# {}",
                meta.emoji, meta.display_name, meta.version, meta.description
            )
        })
        .collect();
    let body = if body.is_empty() {
        vec!["-# *No modules loaded.*".to_string()]
    } else {
        body
    };

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec!["Hub".to_string(), "Modules".to_string()];
    opts.thumbnail_url = guild.icon.clone();
    opts.separator_above_action_rows = true;
    opts.action_rows = vec![tab_row(HubView::Modules)];

    Ok(Card::lines("⚙️ Modules", body)
        .kind(CardKind::Info)
        .options(opts)
        .into_reply())
}

/// Route a module-config interaction.
pub async fn dispatch(
    _services: &Services,
    _guild: &GuildCtx,
    _id: &HubId,
    _data: &serenity::ComponentInteractionData,
    _user_id: u64,
) -> Result<HubOutcome, Error> {
    Ok(HubOutcome::Done)
}
