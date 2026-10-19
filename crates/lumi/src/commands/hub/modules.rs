use poise::serenity_prelude as serenity;
use serenity::all::{
    ButtonStyle, ComponentInteractionDataKind, CreateActionRow, CreateButton,
};

use lumi_kernel::bot::Error;
use lumi_kernel::module::{ConfigField, ConfigFieldKind};
use lumi_kernel::module_state::{self, FeatureRow};
use lumi_kernel::Services;
use lumi_ui::panels::{action_row_of, setting_row, string_select};
use lumi_ui::{format_page_footer, Card, CardKind, CardOptions, CardReply, PageFooterHint};

use super::{cid, tab_row, GuildCtx, HubId, HubOutcome, HubView};

const FEATURES_PER_PAGE: usize = 8;
const FIELDS_PER_PAGE: usize = 8;
const MAX_SELECT_OPTIONS: usize = 25;
/// Separator between field key and value inside a select option value; Discord
/// option values are opaque strings, so this keeps both halves in one value.
const SELECT_SEP: char = '\u{1f}';

/// The feature/module list.
pub async fn list_card(
    services: &Services,
    guild: &GuildCtx,
    page: usize,
    user_id: u64,
) -> Result<CardReply, Error> {
    let all = module_state::list_feature_rows(&services.registry, &services.db, guild.id).await;
    let total_pages = all.len().div_ceil(FEATURES_PER_PAGE).max(1);
    let safe_page = page.min(total_pages - 1);
    let start = safe_page * FEATURES_PER_PAGE;
    let page_rows: Vec<&FeatureRow> = all.iter().skip(start).take(FEATURES_PER_PAGE).collect();

    let mut sections = Vec::with_capacity(page_rows.len());
    for row in &page_rows {
        let configure = CreateButton::new(cid(&["cfg", "open", row.name.as_str()]))
            .label("Configure")
            .emoji('⚙')
            .style(ButtonStyle::Secondary);
        let text = format!(
            "{} **{}** `v{}`\n-# {} - {}",
            row.emoji,
            row.display_name,
            row.version,
            if row.enabled { "Enabled" } else { "Disabled" },
            row.description
        );
        sections.push(setting_row(text, configure));
    }

    let mut action_rows: Vec<CreateActionRow<'static>> = Vec::new();
    let toggles: Vec<CreateButton<'static>> = page_rows
        .iter()
        .map(|row| {
            CreateButton::new(cid(&["cfg", "toggle", row.name.as_str()]))
                .label(cut(&row.display_name, 80))
                .style(if row.enabled {
                    ButtonStyle::Success
                } else {
                    ButtonStyle::Secondary
                })
        })
        .collect();
    for chunk in toggles.chunks(5) {
        action_rows.push(action_row_of(chunk.to_vec()));
    }
    if let Some(pagination) = lumi_ui::panels::pagination_row(
        &cid(&["cfg", "list"]),
        safe_page,
        total_pages,
    ) {
        action_rows.push(pagination);
    }
    action_rows.push(tab_row(HubView::Modules));

    let body = if all.is_empty() {
        vec!["-# *No modules loaded.*".to_string()]
    } else {
        Vec::new()
    };

    let footer = if total_pages > 1 {
        format_page_footer(
            safe_page,
            total_pages,
            Some(PageFooterHint::Hint(
                "Open a module to configure it.".to_string(),
            )),
        )
    } else {
        "Open a module to configure it.".to_string()
    };

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec!["Hub".to_string(), "Modules".to_string()];
    opts.thumbnail_url = guild.icon.clone();
    opts.footer = Some(footer);
    opts.separator_above_action_rows = true;
    opts.sections = sections;
    opts.action_rows = action_rows;

    let _ = user_id;
    Ok(Card::lines("⚙️ Modules", body)
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

/// One rendered schema section: a `group` (or a page of a large group) and its
/// fields, matching lumi's sectioned module detail view.
struct Section<'a> {
    name: Option<String>,
    fields: Vec<&'a ConfigField>,
}

/// Group schema fields by `group`, preserving order, and split any group larger
/// than a page so a section never overflows the card.
fn sections(schema: &[ConfigField]) -> Vec<Section<'_>> {
    let mut groups: Vec<(Option<String>, Vec<&ConfigField>)> = Vec::new();
    for field in schema {
        match groups.iter_mut().find(|(group, _)| group == &field.group) {
            Some((_, fields)) => fields.push(field),
            None => groups.push((field.group.clone(), vec![field])),
        }
    }

    let mut out = Vec::new();
    for (name, fields) in groups {
        let pages = fields.chunks(FIELDS_PER_PAGE).count();
        for (index, chunk) in fields.chunks(FIELDS_PER_PAGE).enumerate() {
            let label = match (&name, pages, index) {
                (_, 1, _) => name.clone(),
                (Some(name), _, index) => Some(format!("{name} ({})", index + 1)),
                (None, _, index) => Some(format!("Page {}", index + 1)),
            };
            out.push(Section {
                name: label,
                fields: chunk.to_vec(),
            });
        }
    }
    out
}

/// The section index containing `key`, so a field edit reopens its own section.
fn section_of(schema: &[ConfigField], key: &str) -> usize {
    sections(schema)
        .iter()
        .position(|section| section.fields.iter().any(|field| field.key == key))
        .unwrap_or(0)
}

/// Picker options: one per section, labelled with its setting count.
fn section_options(sections: &[Section<'_>], current: usize) -> Vec<(String, String)> {
    sections
        .iter()
        .take(MAX_SELECT_OPTIONS)
        .enumerate()
        .map(|(index, section)| {
            let name = section.name.as_deref().unwrap_or("Settings");
            let marker = if index == current { "\u{2022} " } else { "" };
            (
                cut(&format!("{marker}{name} ({} setting(s))", section.fields.len()), 100),
                index.to_string(),
            )
        })
        .collect()
}

/// A module's config panel, one schema section at a time: values plus inline
/// editors for enums/booleans, a section picker and prev/next arrows.
pub async fn config_card(
    services: &Services,
    guild: &GuildCtx,
    module_name: &str,
    section: usize,
) -> Result<CardReply, Error> {
    let Some(module) = services.registry.get(module_name) else {
        return Ok(
            Card::error("Unknown Module", format!("No module named `{module_name}`.")).into_reply(),
        );
    };
    let meta = module.metadata();
    let schema = module.config_schema();
    let overrides = module_state::get_config(&services.db, guild.id, module_name).await;

    let sections = sections(&schema);
    let total = sections.len().max(1);
    let safe = section.min(total - 1);
    let current = &sections[safe];
    let multi = total > 1;

    let mut body = Vec::new();
    if multi {
        if let Some(name) = &current.name {
            body.push(format!("-# Section **{name}** · {}/{}", safe + 1, total));
        }
    }
    if current.fields.is_empty() {
        body.push("-# *No configurable fields.*".to_string());
    } else {
        for &field in &current.fields {
            let value = module_state::format_field_value(field, overrides.get(&field.key));
            let line = format!("**{}** = {value}", field.label);
            if field.description.is_empty() {
                body.push(line);
            } else {
                body.push(format!("{line}\n-# {}", field.description));
            }
        }
    }

    let edit_options = edit_options(&current.fields);
    let mut action_rows: Vec<CreateActionRow<'static>> = Vec::new();
    if !edit_options.is_empty() {
        action_rows.push(string_select(
            cid(&["cfg", "field", module_name]),
            "Edit a setting…",
            edit_options,
        ));
    }
    if multi {
        action_rows.push(string_select(
            cid(&["cfg", "section", module_name]),
            "Jump to a section…",
            section_options(&sections, safe),
        ));
    }
    action_rows.push(action_row_of(vec![
        CreateButton::new(cid(&["cfg", "reset", module_name]))
            .label("Reset Override")
            .emoji('🗑')
            .style(ButtonStyle::Secondary),
        CreateButton::new(cid(&["cfg", "back"]))
            .label("Back")
            .emoji('⬅')
            .style(ButtonStyle::Secondary),
    ]));
    if let Some(pagination) = lumi_ui::panels::pagination_row(
        &cid(&["cfg", "page", module_name]),
        safe,
        total,
    ) {
        action_rows.push(pagination);
    }
    action_rows.push(tab_row(HubView::Modules));

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec!["Hub".to_string(), "Modules".to_string()];
    opts.subtitle = Some(meta.description.clone());
    opts.separator_above_action_rows = true;
    opts.action_rows = action_rows;

    Ok(Card::lines(
        format!("⚙️ {} Config", meta.display_name),
        body,
    )
    .kind(CardKind::Info)
    .options(opts)
    .into_reply())
}

/// Select options that set an enum or boolean field directly (no modal/state).
fn edit_options(fields: &[&ConfigField]) -> Vec<(String, String)> {
    let mut options = Vec::new();
    for field in fields {
        match &field.kind {
            ConfigFieldKind::Enum(choices) => {
                for choice in choices {
                    if options.len() >= MAX_SELECT_OPTIONS {
                        return options;
                    }
                    options.push((
                        cut(&format!("{}: {}", field.label, choice), 100),
                        encode(&field.key, choice),
                    ));
                }
            }
            ConfigFieldKind::Boolean => {
                if options.len() + 2 > MAX_SELECT_OPTIONS {
                    return options;
                }
                options.push((
                    cut(&format!("{}: On", field.label), 100),
                    encode(&field.key, "true"),
                ));
                options.push((
                    cut(&format!("{}: Off", field.label), 100),
                    encode(&field.key, "false"),
                ));
            }
            _ => {}
        }
    }
    options
}

fn encode(key: &str, value: &str) -> String {
    format!("{key}{SELECT_SEP}{value}")
}

fn decode(raw: &str) -> Option<(&str, &str)> {
    raw.split_once(SELECT_SEP)
}

fn select_value(data: &serenity::ComponentInteractionData) -> Option<String> {
    match &data.kind {
        ComponentInteractionDataKind::StringSelect { values } => values.first().cloned(),
        _ => None,
    }
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

/// Route a module-list / config interaction.
///
/// `dispatch_ext` in `mod.rs` only forwards the `cfg` action to this module, so
/// every id is `lumi:cfg:<verb>:<module>[:<arg>]` and the verb is `id.sub()`.
pub async fn dispatch(
    services: &Services,
    guild: &GuildCtx,
    id: &HubId,
    data: &serenity::ComponentInteractionData,
    user_id: u64,
) -> Result<HubOutcome, Error> {
    let name = id.tail.get(1).map(String::as_str);
    match id.sub() {
        Some("list") => {
            let page = id.tail.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
            Ok(HubOutcome::Update(
                list_card(services, guild, page, user_id).await?,
            ))
        }
        Some("toggle") => {
            let Some(name) = name else {
                return Ok(HubOutcome::Done);
            };
            let enabled = module_state::is_enabled(&services.db, guild.id, name).await;
            module_state::set_enabled(&services.db, guild.id, name, !enabled).await?;
            Ok(HubOutcome::Update(
                list_card(services, guild, 0, user_id).await?,
            ))
        }
        Some("open") => {
            let Some(name) = name else {
                return Ok(HubOutcome::Done);
            };
            Ok(HubOutcome::Update(
                config_card(services, guild, name, 0).await?,
            ))
        }
        Some("page") => {
            let Some(name) = name else {
                return Ok(HubOutcome::Done);
            };
            let page = id.tail.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
            Ok(HubOutcome::Update(
                config_card(services, guild, name, page).await?,
            ))
        }
        Some("section") => {
            let Some(name) = name else {
                return Ok(HubOutcome::Done);
            };
            let Some(section) = select_value(data).and_then(|value| value.parse().ok()) else {
                return Ok(HubOutcome::Done);
            };
            Ok(HubOutcome::Update(
                config_card(services, guild, name, section).await?,
            ))
        }
        Some("field") => {
            let Some(name) = name else {
                return Ok(HubOutcome::Done);
            };
            let Some(selected) = select_value(data) else {
                return Ok(HubOutcome::Done);
            };
            let Some((key, raw)) = decode(&selected) else {
                return Ok(HubOutcome::Done);
            };
            let Some(module) = services.registry.get(name) else {
                return Ok(HubOutcome::Done);
            };
            let schema = module.config_schema();
            let Some(field) = schema.iter().find(|f| f.key == key) else {
                return Ok(HubOutcome::Done);
            };
            let section = section_of(&schema, key);
            module_state::set_config_value(&services.db, guild.id, name, key, raw, &field.kind)
                .await?;
            Ok(HubOutcome::Update(
                config_card(services, guild, name, section).await?,
            ))
        }
        Some("reset") => {
            let Some(name) = name else {
                return Ok(HubOutcome::Done);
            };
            module_state::clear_config(&services.db, guild.id, name).await?;
            Ok(HubOutcome::Update(
                config_card(services, guild, name, 0).await?,
            ))
        }
        Some("back") => Ok(HubOutcome::Update(
            list_card(services, guild, 0, user_id).await?,
        )),
        _ => Ok(HubOutcome::Done),
    }
}
