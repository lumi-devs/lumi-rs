use lumi_kernel::Services;
use lumi_kernel::bot::Error;
use lumi_ui::{Card, CardKind, CardOptions, CardReply};

use super::{GuildCtx, HubView, tab_row};

/// The hub landing card.
pub async fn card(services: &Services, guild: &GuildCtx) -> Result<CardReply, Error> {
    let prefix = lumi_kernel::prefix::get(&services.cache, &services.db, guild.id)
        .await
        .unwrap_or_else(|_| ",".to_string());
    let rows =
        lumi_kernel::module_state::list_feature_rows(&services.registry, &services.db, guild.id)
            .await;
    let total = rows.len();
    let enabled = rows.iter().filter(|r| r.enabled).count();

    let hints = [
        "⚙️ **Modules** - enable, disable, and configure every feature",
        "🛡️ **Permissions** - permit grants and per-command overrides",
        "🏰 **Settings** - server command prefix",
        "📦 **Addons** - extend Lumi with add-on modules",
    ]
    .join("\n");

    let body = vec![
        "Manage everything for this server from one place - no scattered commands to remember."
            .to_string(),
        format!("⚙️ **{enabled}** of **{total}** modules enabled"),
        format!("🏰 Prefix `{prefix}`"),
        hints,
    ];

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec!["Hub".to_string()];
    opts.thumbnail_url = guild.icon.clone();
    opts.footer = Some("Select a tab below to continue.".to_string());
    opts.separator_above_action_rows = true;
    opts.action_rows = vec![tab_row(HubView::Home)];

    Ok(Card::lines("🤖 Lumi Control Panel", body)
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}
