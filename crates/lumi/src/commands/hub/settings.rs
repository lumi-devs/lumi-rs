use poise::serenity_prelude as serenity;
use serenity::all::{
    CreateButton, CreateInputText, CreateLabel, CreateModal, CreateModalComponent, InputTextStyle,
    ModalInteraction,
};

use lumi_kernel::Services;
use lumi_kernel::bot::Error;
use lumi_ui::panels::{action_row_of, setting_row};
use lumi_ui::{Card, CardKind, CardOptions, CardReply};

use super::{GuildCtx, HubOutcome, HubView, cid, tab_row};

/// The server settings card.
pub async fn card(
    services: &Services,
    guild: &GuildCtx,
    _user_id: u64,
) -> Result<CardReply, Error> {
    let prefix = lumi_kernel::prefix::get(&services.cache, &services.db, guild.id)
        .await
        .unwrap_or_else(|_| ",".to_string());
    let is_default = prefix == ",";

    let edit = CreateButton::new(cid(&["prefix", "set"]))
        .label("Edit")
        .emoji('✏')
        .style(serenity::ButtonStyle::Primary);
    let prefix_line = format!("💻 **Prefix** - `{prefix}`");
    let prefix_hint =
        "-# Prefix commands work for a curated set of moderation and utility commands.";
    let prefix_section = setting_row(format!("{prefix_line}\n{prefix_hint}"), edit);

    let reset = CreateButton::new(cid(&["prefix", "reset"]))
        .label("Reset")
        .emoji('🗑')
        .style(serenity::ButtonStyle::Secondary)
        .disabled(is_default);
    let update_all = CreateButton::new(cid(&["update_all"]))
        .label("Update Addons")
        .emoji('🔄')
        .style(serenity::ButtonStyle::Secondary);

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec!["Hub".to_string(), "Settings".to_string()];
    opts.thumbnail_url = guild.icon.clone();
    opts.separator_above_action_rows = true;
    opts.sections = vec![prefix_section];
    opts.action_rows = vec![
        action_row_of(vec![reset, update_all]),
        tab_row(HubView::Settings),
    ];

    Ok(Card::lines("🏰 Server Settings", vec![])
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

/// The set-prefix modal.
pub fn prefix_modal() -> CreateModal<'static> {
    CreateModal::new(super::PREFIX_MODAL_ID, "Set Command Prefix").components(vec![
        CreateModalComponent::Label(CreateLabel::input_text(
            "New prefix",
            CreateInputText::new(InputTextStyle::Short, "prefix")
                .max_length(5)
                .required(true)
                .placeholder("e.g. ,"),
        )),
    ])
}

/// Persist a submitted prefix and re-render settings.
pub async fn handle_prefix_modal(
    services: &Services,
    guild: &GuildCtx,
    modal: &ModalInteraction,
) -> Result<HubOutcome, Error> {
    let raw = super::read_input(&modal.data.components, "prefix").unwrap_or_default();
    let prefix = raw.trim();
    if prefix.is_empty() || prefix.chars().count() > 5 {
        return Ok(HubOutcome::Ephemeral(
            Card::error(
                "Invalid Prefix",
                "A prefix must be between 1 and 5 characters.",
            )
            .into_reply(),
        ));
    }
    lumi_db::set_prefix(&services.db, guild.id, prefix).await?;
    lumi_kernel::prefix::invalidate(&services.cache, guild.id).await?;
    let card = card(services, guild, modal.user.id.get()).await?;
    Ok(HubOutcome::Update(card))
}
