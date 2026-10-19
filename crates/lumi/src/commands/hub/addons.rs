use poise::serenity_prelude as serenity;
use serenity::all::ModalInteraction;

use lumi_kernel::bot::Error;
use lumi_kernel::Services;
use lumi_ui::{Card, CardKind, CardOptions, CardReply};

use super::{tab_row, GuildCtx, HubId, HubOutcome, HubView};

/// The add-ons dashboard.
pub async fn dashboard_card(
    services: &Services,
    guild: &GuildCtx,
    _user_id: u64,
) -> Result<CardReply, Error> {
    let repos = services
        .downloader
        .list_repos()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    let body: Vec<String> = if repos.is_empty() {
        vec!["-# *No add-on repositories are added.*".to_string()]
    } else {
        repos
            .iter()
            .map(|repo| {
                let commit = repo
                    .commit
                    .as_deref()
                    .map(|c| c.chars().take(7).collect::<String>())
                    .unwrap_or_else(|| "unknown".to_string());
                format!("📦 **{}** — branch `{}` — `{}`", repo.name, repo.branch, commit)
            })
            .collect()
    };

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec!["Hub".to_string(), "Addons".to_string()];
    opts.thumbnail_url = guild.icon.clone();
    opts.separator_above_action_rows = true;
    opts.action_rows = vec![tab_row(HubView::Addons)];

    Ok(Card::lines("📦 Add-ons", body)
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

/// Route an add-on interaction.
pub async fn dispatch(
    _services: &Services,
    _guild: &GuildCtx,
    _id: &HubId,
    _data: &serenity::ComponentInteractionData,
    _user_id: u64,
) -> Result<HubOutcome, Error> {
    Ok(HubOutcome::Done)
}

/// Handle an add-on management modal submission.
pub async fn handle_modal(
    _services: &Services,
    _guild: &GuildCtx,
    _modal: &ModalInteraction,
    _action: &str,
) -> Result<HubOutcome, Error> {
    Ok(HubOutcome::Done)
}
