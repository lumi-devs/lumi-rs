use std::borrow::Cow;

use poise::serenity_prelude as serenity;
use serde_json::Value;
use serenity::all::{
    ButtonStyle, ComponentInteractionDataKind, CreateActionRow, CreateButton, CreateInputText,
    CreateLabel, CreateModal, CreateModalComponent, CreateSection, InputTextStyle,
    ModalInteraction,
};

use lumi_kernel::Services;
use lumi_kernel::bot::Error;
use lumi_kernel::module::{ConfigField, ConfigFieldKind};
use lumi_kernel::module_state::{self, FeatureRow};
use lumi_ui::panels::{
    action_row_of, back_button, channel_select, pagination_row, role_select, role_select_multi,
    setting_row, string_select, user_select,
};
use lumi_ui::{Card, CardKind, CardOptions, CardReply, cut_text, format_page_footer};

use super::{GuildCtx, HubId, HubOutcome, HubView, cid, tab_row};

/// Page size limits to keep total components within Discord card limits.
const FEATURES_PER_PAGE: usize = 4;
const FIELDS_PER_PAGE: usize = 5;
const MAX_SELECT_OPTIONS: usize = 25;
const MAX_MULTI_VALUES: u8 = 25;
/// Custom-id prefix of the field-edit modal (`lumi:cfgmodal:<module>:<key>:<section>`).
const MODAL_PREFIX: &str = "cfgmodal";

/// The paginated module list.
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
    if let Some(pagination) = pagination_row(&cid(&["cfg", "list"]), safe_page, total_pages) {
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
            Some("Open a module to configure it."),
        )
    } else {
        "Open a module to configure it.".to_string()
    };

    let opts = CardOptions {
        breadcrumbs: vec!["Hub".to_string(), "Modules".to_string()],
        thumbnail_url: guild.icon.clone(),
        footer: Some(footer),
        separator_above_action_rows: true,
        sections,
        action_rows,
        ..CardOptions::default()
    };

    let _ = user_id;
    Ok(Card::lines("⚙️ Modules", body)
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

/// Rendered schema section and its fields.
struct Section<'a> {
    name: Option<Cow<'a, str>>,
    fields: Vec<&'a ConfigField>,
}

/// Group schema fields by group and paginate large groups.
fn sections(schema: &[ConfigField]) -> Vec<Section<'_>> {
    let mut groups: Vec<(Option<&str>, Vec<&ConfigField>)> = Vec::new();
    for field in schema {
        let field_group = field.group.as_deref();
        match groups.iter_mut().find(|(group, _)| *group == field_group) {
            Some((_, fields)) => fields.push(field),
            None => groups.push((field_group, vec![field])),
        }
    }

    let mut out = Vec::new();
    for (name, fields) in groups {
        let pages = fields.chunks(FIELDS_PER_PAGE).count();
        for (index, chunk) in fields.chunks(FIELDS_PER_PAGE).enumerate() {
            let label = match (name, pages, index) {
                (Some(name), 1, _) => Some(Cow::Borrowed(name)),
                (None, 1, _) => None,
                (Some(name), _, index) => Some(Cow::Owned(format!("{name} ({})", index + 1))),
                (None, _, index) => Some(Cow::Owned(format!("Page {}", index + 1))),
            };
            out.push(Section {
                name: label,
                fields: chunk.to_vec(),
            });
        }
    }
    if out.is_empty() {
        out.push(Section {
            name: None,
            fields: Vec::new(),
        });
    }
    out
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
                cut_text(
                    &format!("{marker}{name} ({} setting(s))", section.fields.len()),
                    100,
                ),
                index.to_string(),
            )
        })
        .collect()
}

/// A field's status glyph: check/cross for booleans, dot otherwise.
fn status_glyph(field: &ConfigField, value: Option<&Value>) -> &'static str {
    if matches!(&field.kind, ConfigFieldKind::Boolean) {
        let on = value
            .and_then(Value::as_bool)
            .or_else(|| field.default.as_bool())
            .unwrap_or(false);
        return if on { "✅" } else { "❌" };
    }
    if value.is_some_and(|value| !value.is_null()) {
        "🟢"
    } else {
        "🔴"
    }
}

/// One `glyph **Label** = value` line, plus the description when present.
fn field_lines(field: &ConfigField, value: Option<&Value>) -> String {
    let rendered = module_state::format_field_value(field, value);
    let line = format!(
        "{} **{}** = {rendered}",
        status_glyph(field, value),
        field.label
    );
    if field.description.is_empty() {
        line
    } else {
        format!("{line}\n-# {}", field.description)
    }
}

/// One field row with its editor control.
fn field_editor(
    module_name: &str,
    field: &ConfigField,
    section: usize,
    value: Option<&Value>,
) -> CreateSection<'static> {
    let section_token = section.to_string();
    let accessory = if matches!(field.kind, ConfigFieldKind::Boolean) {
        let on = value
            .and_then(Value::as_bool)
            .or_else(|| field.default.as_bool())
            .unwrap_or(false);
        CreateButton::new(cid(&[
            "cfg",
            "bool",
            module_name,
            field.key.as_str(),
            section_token.as_str(),
        ]))
        .label(if on { "✅" } else { "❌" })
        .style(if on {
            ButtonStyle::Success
        } else {
            ButtonStyle::Secondary
        })
    } else {
        CreateButton::new(cid(&[
            "cfg",
            "fedit",
            module_name,
            field.key.as_str(),
            section_token.as_str(),
        ]))
        .label("Edit")
        .emoji('✏')
        .style(ButtonStyle::Secondary)
    };
    setting_row(field_lines(field, value), accessory)
}

/// Toggle, reset, history, and back controls for a module panel.
fn config_controls(module_name: &str, enabled: bool, section: usize) -> CreateActionRow<'static> {
    let section_token = section.to_string();
    action_row_of(vec![
        CreateButton::new(cid(&["cfg", "toggle", module_name]))
            .label(if enabled {
                "Disable Module"
            } else {
                "Enable Module"
            })
            .emoji(if enabled { '❌' } else { '✅' })
            .style(if enabled {
                ButtonStyle::Danger
            } else {
                ButtonStyle::Success
            }),
        CreateButton::new(cid(&["cfg", "reset", module_name]))
            .label("Reset")
            .emoji('🗑')
            .style(ButtonStyle::Secondary),
        CreateButton::new(cid(&["cfg", "hist", module_name, section_token.as_str()]))
            .label("History")
            .emoji('🕐')
            .style(ButtonStyle::Secondary),
        CreateButton::new(cid(&["cfg", "back"]))
            .label("Back")
            .emoji('⬅')
            .style(ButtonStyle::Secondary),
    ])
}

/// A module's config panel: per-field editors, section picker, toggle, reset and pager.
pub async fn config_card(
    services: &Services,
    guild: &GuildCtx,
    module_name: &str,
    section: usize,
) -> Result<CardReply, Error> {
    let Some(module) = services.registry.get(module_name) else {
        return Ok(Card::error(
            "Unknown Module",
            format!("No module named `{module_name}`."),
        )
        .into_reply());
    };
    let meta = module.metadata();
    let schema = module.config_schema();
    let overrides = module_state::get_config(&services.db, guild.id, module_name).await;
    let enabled = module_state::is_enabled(&services.db, guild.id, module_name).await;

    let sections = sections(&schema);
    let total = sections.len().max(1);
    let safe = section.min(total - 1);
    let current = sections.get(safe);
    let multi = total > 1;

    let mut body = vec![format!(
        "**Status:** {}",
        if enabled {
            "🟢 `ENABLED`"
        } else {
            "🔴 `DISABLED`"
        }
    )];
    if multi && let Some(name) = current.and_then(|s| s.name.as_deref()) {
        body.push(format!("-# Section **{name}** · {}/{}", safe + 1, total));
    }

    let fields: &[&ConfigField] = current.map_or(&[], |s| s.fields.as_slice());
    if fields.is_empty() {
        body.push("-# *No configurable fields.*".to_string());
    }

    let sections_ui: Vec<_> = fields
        .iter()
        .map(|&field| field_editor(module_name, field, safe, overrides.get(&field.key)))
        .collect();

    let mut action_rows: Vec<CreateActionRow<'static>> = Vec::new();
    if multi {
        action_rows.push(string_select(
            cid(&["cfg", "section", module_name]),
            "Jump to a section…",
            section_options(&sections, safe),
        ));
    }
    action_rows.push(config_controls(module_name, enabled, safe));
    if let Some(pagination) = pagination_row(&cid(&["cfg", "page", module_name]), safe, total) {
        action_rows.push(pagination);
    }
    action_rows.push(tab_row(HubView::Modules));

    let opts = CardOptions {
        breadcrumbs: vec![
            "Hub".to_string(),
            "Modules".to_string(),
            meta.display_name.clone(),
        ],
        subtitle: Some(meta.description.clone()),
        separator_above_action_rows: true,
        sections: sections_ui,
        action_rows,
        ..CardOptions::default()
    };

    Ok(Card::lines(format!("⚙️ {}", meta.display_name), body)
        .kind(if enabled {
            CardKind::Primary
        } else {
            CardKind::Warning
        })
        .options(opts)
        .into_reply())
}

/// Native select or modal button for one field's type.
fn field_editor_rows(
    field: &ConfigField,
    module_name: &str,
    key: &str,
    section_token: &str,
) -> Vec<CreateActionRow<'static>> {
    match &field.kind {
        ConfigFieldKind::Enum(choices) => {
            let options = choices
                .iter()
                .take(MAX_SELECT_OPTIONS)
                .map(|choice| (cut_text(choice, 100), choice.clone()))
                .collect();
            vec![string_select(
                cid(&["cfg", "enum", module_name, key, section_token]),
                cut_text(&field.label, 100),
                options,
            )]
        }
        ConfigFieldKind::Channel => vec![channel_select(
            cid(&["cfg", "setch", module_name, key, section_token]),
            cut_text(&field.label, 100),
        )],
        ConfigFieldKind::Role => vec![role_select(
            cid(&["cfg", "setrole", module_name, key, section_token]),
            cut_text(&field.label, 100),
        )],
        ConfigFieldKind::RoleArray => vec![role_select_multi(
            cid(&["cfg", "setrole", module_name, key, section_token]),
            cut_text(&field.label, 100),
            MAX_MULTI_VALUES,
        )],
        ConfigFieldKind::User => vec![user_select(
            cid(&["cfg", "setuser", module_name, key, section_token]),
            cut_text(&field.label, 100),
        )],
        _ => vec![action_row_of(vec![
            CreateButton::new(cid(&["cfg", "modal", module_name, key, section_token]))
                .label("Enter value…")
                .emoji('✏')
                .style(ButtonStyle::Primary),
        ])],
    }
}

/// A single field's editor: a native select for enums/roles/users/channels, or a
/// modal button for free-form text.
async fn field_edit_card(
    services: &Services,
    guild: &GuildCtx,
    module_name: &str,
    key: &str,
    section: usize,
) -> Result<CardReply, Error> {
    let Some(module) = services.registry.get(module_name) else {
        return Ok(Card::error(
            "Unknown Module",
            format!("No module named `{module_name}`."),
        )
        .into_reply());
    };
    let meta = module.metadata();
    let schema = module.config_schema();
    let Some(field) = schema.iter().find(|field| field.key == key) else {
        return Ok(
            Card::error("Unknown Setting", format!("No setting named `{key}`.")).into_reply(),
        );
    };
    let overrides = module_state::get_config(&services.db, guild.id, module_name).await;
    let value = overrides.get(key);

    let section_token = section.to_string();
    let mut rows = field_editor_rows(field, module_name, key, &section_token);
    rows.push(back_button(cid(&[
        "cfg",
        "open",
        module_name,
        &section_token,
    ])));

    let body = vec![
        module_state::format_field_value(field, value),
        "-# Pick a new value below, or clear the selection to unset.".to_string(),
    ];

    let opts = CardOptions {
        breadcrumbs: vec![
            "Hub".to_string(),
            "Modules".to_string(),
            meta.display_name.clone(),
            field.label.clone(),
        ],
        subtitle: if field.description.is_empty() {
            None
        } else {
            Some(field.description.clone())
        },
        separator_above_action_rows: true,
        action_rows: rows,
        ..CardOptions::default()
    };

    Ok(
        Card::lines(format!("⚙️ {} • {}", meta.display_name, field.label), body)
            .kind(CardKind::Primary)
            .options(opts)
            .into_reply(),
    )
}

fn history_card(services: &Services, module_name: &str, section: usize) -> CardReply {
    let Some(module) = services.registry.get(module_name) else {
        return Card::error(
            "Unknown Module",
            format!("No module named `{module_name}`."),
        )
        .into_reply();
    };
    let meta = module.metadata();
    let body = vec![
        "-# Recent configuration changes recorded for this server.".to_string(),
        "*No changes recorded yet.*".to_string(),
    ];

    let opts = CardOptions {
        breadcrumbs: vec![
            "Hub".to_string(),
            "Modules".to_string(),
            meta.display_name.clone(),
            "History".to_string(),
        ],
        separator_above_action_rows: true,
        action_rows: vec![back_button(cid(&[
            "cfg",
            "open",
            module_name,
            &section.to_string(),
        ]))],
        ..CardOptions::default()
    };

    Card::lines(format!("🕐 {} • History", meta.display_name), body)
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply()
}

fn overrides_card(services: &Services, module_name: &str, section: usize) -> CardReply {
    let Some(module) = services.registry.get(module_name) else {
        return Card::error(
            "Unknown Module",
            format!("No module named `{module_name}`."),
        )
        .into_reply();
    };
    let meta = module.metadata();
    let body = vec![
        "-# Channel, role, and category specific configuration overrides.".to_string(),
        "*No channel, role, or user overrides configured.*".to_string(),
    ];

    let opts = CardOptions {
        breadcrumbs: vec![
            "Hub".to_string(),
            "Modules".to_string(),
            meta.display_name.clone(),
            "Overrides".to_string(),
        ],
        separator_above_action_rows: true,
        action_rows: vec![back_button(cid(&[
            "cfg",
            "open",
            module_name,
            &section.to_string(),
        ]))],
        ..CardOptions::default()
    };

    Card::lines(format!("🛡️ {} • Overrides", meta.display_name), body)
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply()
}

fn field_modal(module_name: &str, field: &ConfigField, section: usize) -> CreateModal<'static> {
    CreateModal::new(
        cid(&[
            MODAL_PREFIX,
            module_name,
            field.key.as_str(),
            &section.to_string(),
        ]),
        format!("Set {}", cut_text(&field.label, 45)),
    )
    .components(vec![CreateModalComponent::Label(CreateLabel::input_text(
        cut_text(&field.label, 45),
        CreateInputText::new(InputTextStyle::Paragraph, "value")
            .required(false)
            .max_length(4000)
            .placeholder(cut_text(&field.description, 100)),
    ))])
}

fn select_string(data: &serenity::ComponentInteractionData) -> Option<String> {
    if let ComponentInteractionDataKind::StringSelect { values } = &data.kind {
        values.first().cloned()
    } else {
        None
    }
}

/// Every selected snowflake id from a user/role/channel select, in order.
fn select_ids(data: &serenity::ComponentInteractionData) -> Vec<String> {
    match &data.kind {
        ComponentInteractionDataKind::UserSelect { values } => {
            values.iter().map(|id| id.get().to_string()).collect()
        }
        ComponentInteractionDataKind::RoleSelect { values } => {
            values.iter().map(|id| id.get().to_string()).collect()
        }
        ComponentInteractionDataKind::ChannelSelect { values } => {
            values.iter().map(|id| id.get().to_string()).collect()
        }
        _ => Vec::new(),
    }
}

/// Route module-list and config interactions.
pub async fn dispatch(
    services: &Services,
    guild: &GuildCtx,
    id: &HubId,
    data: &serenity::ComponentInteractionData,
    user_id: u64,
) -> Result<HubOutcome, Error> {
    match id.get(1) {
        Some("list" | "toggle" | "open" | "page" | "section") => {
            dispatch_nav(services, guild, id, data, user_id).await
        }
        Some("fedit" | "modal") => dispatch_editor(services, guild, id).await,
        Some("bool") => dispatch_bool(services, guild, id).await,
        Some("enum" | "setch" | "setrole" | "setuser") => {
            dispatch_value(services, guild, id, data).await
        }
        Some("reset" | "hist" | "ovr" | "back") => {
            dispatch_meta(services, guild, id, user_id).await
        }
        _ => Ok(HubOutcome::Done),
    }
}

async fn dispatch_nav(
    services: &Services,
    guild: &GuildCtx,
    id: &HubId,
    data: &serenity::ComponentInteractionData,
    user_id: u64,
) -> Result<HubOutcome, Error> {
    let name = id.get(2);
    match id.get(1) {
        Some("list") => {
            let page = id.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
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
            let section = id.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            Ok(HubOutcome::Update(
                config_card(services, guild, name, section).await?,
            ))
        }
        Some("page") => {
            let Some(name) = name else {
                return Ok(HubOutcome::Done);
            };
            let page = id.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            Ok(HubOutcome::Update(
                config_card(services, guild, name, page).await?,
            ))
        }
        Some("section") => {
            let Some(name) = name else {
                return Ok(HubOutcome::Done);
            };
            let Some(section) = select_string(data).and_then(|value| value.parse().ok()) else {
                return Ok(HubOutcome::Done);
            };
            Ok(HubOutcome::Update(
                config_card(services, guild, name, section).await?,
            ))
        }
        _ => Ok(HubOutcome::Done),
    }
}

async fn dispatch_editor(
    services: &Services,
    guild: &GuildCtx,
    id: &HubId,
) -> Result<HubOutcome, Error> {
    let (Some(name), Some(key)) = (id.get(2), id.get(3)) else {
        return Ok(HubOutcome::Done);
    };
    let section = id.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
    match id.get(1) {
        Some("fedit") => Ok(HubOutcome::Update(
            field_edit_card(services, guild, name, key, section).await?,
        )),
        Some("modal") => {
            let Some(module) = services.registry.get(name) else {
                return Ok(HubOutcome::Done);
            };
            let schema = module.config_schema();
            let Some(field) = schema.iter().find(|field| field.key == key) else {
                return Ok(HubOutcome::Done);
            };
            Ok(HubOutcome::Modal(field_modal(name, field, section)))
        }
        _ => Ok(HubOutcome::Done),
    }
}

async fn dispatch_bool(
    services: &Services,
    guild: &GuildCtx,
    id: &HubId,
) -> Result<HubOutcome, Error> {
    let (Some(name), Some(key)) = (id.get(2), id.get(3)) else {
        return Ok(HubOutcome::Done);
    };
    let section = id.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
    let Some(module) = services.registry.get(name) else {
        return Ok(HubOutcome::Done);
    };
    let schema = module.config_schema();
    let Some(field) = schema.iter().find(|field| field.key == key) else {
        return Ok(HubOutcome::Done);
    };
    let current = module_state::get_config(&services.db, guild.id, name)
        .await
        .get(key)
        .and_then(Value::as_bool)
        .or_else(|| field.default.as_bool())
        .unwrap_or(false);
    module_state::set_config_field_cached(
        &services.cache,
        &services.db,
        guild.id,
        name,
        key,
        Value::Bool(!current),
    )
    .await?;
    Ok(HubOutcome::Update(
        config_card(services, guild, name, section).await?,
    ))
}

async fn dispatch_value(
    services: &Services,
    guild: &GuildCtx,
    id: &HubId,
    data: &serenity::ComponentInteractionData,
) -> Result<HubOutcome, Error> {
    let (Some(name), Some(key)) = (id.get(2), id.get(3)) else {
        return Ok(HubOutcome::Done);
    };
    let section = id.get(4).and_then(|s| s.parse().ok()).unwrap_or(0);
    if id.get(1) == Some("enum") {
        let Some(choice) = select_string(data) else {
            return Ok(HubOutcome::Done);
        };
        let Some(module) = services.registry.get(name) else {
            return Ok(HubOutcome::Done);
        };
        let schema = module.config_schema();
        let Some(field) = schema.iter().find(|field| field.key == key) else {
            return Ok(HubOutcome::Done);
        };
        module_state::set_config_value_cached(
            &services.cache,
            &services.db,
            guild.id,
            name,
            key,
            &choice,
            &field.kind,
        )
        .await?;
        return Ok(HubOutcome::Update(
            config_card(services, guild, name, section).await?,
        ));
    }

    let ids = select_ids(data);
    let value = match ids.first() {
        None => Value::Null,
        Some(_) if is_role_array(services, name, key) => {
            Value::Array(ids.iter().map(|id| Value::String(id.clone())).collect())
        }
        Some(first) => Value::String(first.clone()),
    };
    module_state::set_config_field_cached(
        &services.cache,
        &services.db,
        guild.id,
        name,
        key,
        value,
    )
    .await?;
    Ok(HubOutcome::Update(
        config_card(services, guild, name, section).await?,
    ))
}

async fn dispatch_meta(
    services: &Services,
    guild: &GuildCtx,
    id: &HubId,
    user_id: u64,
) -> Result<HubOutcome, Error> {
    let name = id.get(2);
    match id.get(1) {
        Some("reset") => {
            let Some(name) = name else {
                return Ok(HubOutcome::Done);
            };
            module_state::clear_config_cached(&services.cache, &services.db, guild.id, name)
                .await?;
            Ok(HubOutcome::Update(
                config_card(services, guild, name, 0).await?,
            ))
        }
        Some("hist") => {
            let Some(name) = name else {
                return Ok(HubOutcome::Done);
            };
            let section = id.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            Ok(HubOutcome::Update(history_card(services, name, section)))
        }
        Some("ovr") => {
            let Some(name) = name else {
                return Ok(HubOutcome::Done);
            };
            let section = id.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
            Ok(HubOutcome::Update(overrides_card(services, name, section)))
        }
        Some("back") => Ok(HubOutcome::Update(
            list_card(services, guild, 0, user_id).await?,
        )),
        _ => Ok(HubOutcome::Done),
    }
}

/// Whether the field at `key` is a multi-role array (drives single vs array IDs).
fn is_role_array(services: &Services, module_name: &str, key: &str) -> bool {
    services.registry.get(module_name).is_some_and(|module| {
        module
            .config_schema()
            .iter()
            .any(|field| field.key == key && matches!(field.kind, ConfigFieldKind::RoleArray))
    })
}

/// Handle a `lumi:cfgmodal:<module>:<key>:<section>` submission.
pub async fn handle_modal(
    services: &Services,
    guild: &GuildCtx,
    modal: &ModalInteraction,
) -> Result<HubOutcome, Error> {
    let Some(id) = HubId::parse(&modal.data.custom_id, super::PREFIX) else {
        return Ok(HubOutcome::Done);
    };
    if id.get(0) != Some(MODAL_PREFIX) {
        return Ok(HubOutcome::Done);
    }
    let (Some(name), Some(key)) = (id.get(1), id.get(2)) else {
        return Ok(HubOutcome::Done);
    };
    let section = id.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let Some(module) = services.registry.get(name) else {
        return Ok(HubOutcome::Done);
    };
    let schema = module.config_schema();
    let Some(field) = schema.iter().find(|field| field.key == key) else {
        return Ok(HubOutcome::Done);
    };
    let raw = super::read_input(&modal.data.components, "value").unwrap_or_default();
    let Some(value) = parse_modal_value(&raw, &field.kind) else {
        return Ok(HubOutcome::Ephemeral(
            Card::error(
                "Invalid Value",
                format!(
                    "`{}` could not be parsed as {}.",
                    cut_text(&raw, 120),
                    field.label
                ),
            )
            .into_reply(),
        ));
    };
    module_state::set_config_field_cached(
        &services.cache,
        &services.db,
        guild.id,
        name,
        key,
        value,
    )
    .await?;
    Ok(HubOutcome::Update(
        config_card(services, guild, name, section).await?,
    ))
}

/// Convert a modal's text into the field's JSON type, or `None` if invalid.
fn parse_modal_value(raw: &str, kind: &ConfigFieldKind) -> Option<Value> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Some(Value::Null);
    }
    match kind {
        ConfigFieldKind::Integer => trimmed.parse::<i64>().ok().map(Value::from),
        ConfigFieldKind::StringArray => Some(Value::Array(
            trimmed
                .split(['\n', ','])
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(|item| Value::String(item.to_string()))
                .collect(),
        )),
        ConfigFieldKind::ObjectArray => serde_json::from_str::<Value>(trimmed)
            .ok()
            .filter(Value::is_array),
        _ => Some(Value::String(raw.to_string())),
    }
}
