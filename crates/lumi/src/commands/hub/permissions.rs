use poise::serenity_prelude as serenity;
use serenity::all::{ButtonStyle, ComponentInteractionDataKind, CreateActionRow, CreateButton};

use lumi_kernel::Services;
use lumi_kernel::bot::Error;
use lumi_kernel::permits::{ExplainSubject, PermitKind, PermitTargetType, Verdict, explain_permit};
use lumi_ui::panels::{
    action_row_of, back_button, pagination_row, role_select, setting_row, string_select,
    user_select,
};
use lumi_ui::{Card, CardKind, CardOptions, CardReply, format_page_footer};

use super::{GuildCtx, HubId, HubOutcome, HubView, cid, tab_row};

const PERMS_PER_PAGE: usize = 4;
const PERMIT_ORDER_PER_PAGE: usize = 3;
const EXPLAIN_NODES_PER_PAGE: usize = 25;

/// The permits overview: assignments with per-row Revoke, plus grant/order/explain.
pub async fn list_card(
    services: &Services,
    guild: &GuildCtx,
    page: usize,
    _user_id: u64,
) -> Result<CardReply, Error> {
    let assignments = lumi_db::list_assignments(&services.db, guild.id).await?;
    let total = page_count(assignments.len(), PERMS_PER_PAGE);
    let safe_page = page.min(total - 1);
    let start = safe_page
        .saturating_mul(PERMS_PER_PAGE)
        .min(assignments.len());
    let end = start.saturating_add(PERMS_PER_PAGE).min(assignments.len());

    let sections = assignments[start..end]
        .iter()
        .map(|a| {
            let target_type = target_str(a.target_type);
            let lock = if a.builtin { " 🔒" } else { "" };
            let text = format!(
                "{} {}{}\n-# {} · {} {}",
                icon(a.kind),
                a.permit_name,
                lock,
                kind_str(a.kind),
                target_type,
                mention(a.target_type, a.target_id),
            );
            let payload = format!("{}|{}|{}", a.permit_id, target_type, a.target_id);
            let revoke = CreateButton::new(cid(&["permdel", payload.as_str()]))
                .label("Revoke")
                .style(ButtonStyle::Danger);
            setting_row(text, revoke)
        })
        .collect::<Vec<_>>();

    let add_row = action_row_of(vec![
        CreateButton::new(cid(&["permit", "grant", "custom"]))
            .label("Assign Custom…")
            .emoji('✅')
            .style(ButtonStyle::Success),
        CreateButton::new(cid(&["permit", "grant", "enforced"]))
            .label("Assign Enforced…")
            .emoji('🛡')
            .style(ButtonStyle::Primary),
        CreateButton::new(cid(&["permorder", "0"]))
            .label("Order…")
            .emoji('↕')
            .style(ButtonStyle::Secondary),
        CreateButton::new(cid(&["permexplain"]))
            .label("Explain…")
            .emoji('🔍')
            .style(ButtonStyle::Secondary),
    ]);

    let mut action_rows = vec![add_row];
    if let Some(row) = pagination_row(&cid(&["permpage"]), safe_page, total) {
        action_rows.push(row);
    }
    action_rows.push(tab_row(HubView::Permissions));

    let footer = if total > 1 {
        let hint = format!("{} assignment(s)", assignments.len());
        format_page_footer(safe_page, total, Some(&hint))
    } else {
        format!("{} assignment(s)", assignments.len())
    };

    let body = if assignments.is_empty() {
        "*No permits are assigned yet - every command uses its default access.*".to_string()
    } else {
        "-# ✅ custom · enforced. Enforced permits survive anti-nuke quarantine.".to_string()
    };

    let opts = CardOptions {
        breadcrumbs: vec!["Hub".to_string(), "Permissions".to_string()],
        separator_above_action_rows: true,
        sections,
        footer: Some(footer),
        action_rows,
        ..Default::default()
    };

    Ok(Card::lines("🛡️ Permits", vec![body])
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

/// Permit display order with up/down nudge buttons.
async fn order_card(
    services: &Services,
    guild: &GuildCtx,
    page: usize,
) -> Result<CardReply, Error> {
    let permits = lumi_db::list_permits(&services.db, guild.id).await?;
    let total = page_count(permits.len(), PERMIT_ORDER_PER_PAGE);
    let safe_page = page.min(total - 1);
    let start = safe_page * PERMIT_ORDER_PER_PAGE;

    let lines: Vec<String> = permits
        .iter()
        .enumerate()
        .map(|(i, p)| {
            format!(
                "{}. {} {}{}",
                i + 1,
                icon(p.kind),
                p.name,
                if p.builtin { " 🔒" } else { "" }
            )
        })
        .collect();

    let mut action_rows: Vec<CreateActionRow<'static>> = permits
        .iter()
        .enumerate()
        .skip(start)
        .take(PERMIT_ORDER_PER_PAGE)
        .map(|(i, permit)| {
            let up = format!("{}|up", permit.id);
            let down = format!("{}|down", permit.id);
            action_row_of(vec![
                CreateButton::new(cid(&["permmove", up.as_str()]))
                    .label("↑")
                    .style(ButtonStyle::Secondary)
                    .disabled(i == 0),
                CreateButton::new(cid(&["permmove", down.as_str()]))
                    .label("↓")
                    .style(ButtonStyle::Secondary)
                    .disabled(i + 1 == permits.len()),
            ])
        })
        .collect();
    if let Some(row) = pagination_row(&cid(&["permorderpage"]), safe_page, total) {
        action_rows.push(row);
    }
    action_rows.push(tab_row(HubView::Permissions));

    let body = if lines.is_empty() {
        "*No permits yet.*".to_string()
    } else {
        lines.join("\n")
    };

    let opts = CardOptions {
        breadcrumbs: vec![
            "Hub".to_string(),
            "Permissions".to_string(),
            "Order".to_string(),
        ],
        separator_above_action_rows: true,
        action_rows,
        ..Default::default()
    };

    Ok(Card::lines("🛡️ Permit Order", vec![body])
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

/// Pick which permit of `kind` to assign.
async fn picker_card(
    services: &Services,
    guild: &GuildCtx,
    kind: PermitKind,
) -> Result<CardReply, Error> {
    let permits = lumi_db::list_permits_for_kind(&services.db, guild.id, kind).await?;
    let kind_label = if kind == PermitKind::Enforced {
        "Enforced"
    } else {
        "Custom"
    };
    let back = back_button(cid(&["tab", "permissions"]));

    let (body, action_rows) = if permits.is_empty() {
        (
            "No permits of this kind exist yet. Create one from the dashboard.".to_string(),
            vec![back],
        )
    } else {
        let options = permits
            .iter()
            .take(25)
            .map(|p| {
                let label = if p.builtin {
                    format!("{} (built-in)", p.name)
                } else {
                    p.name.clone()
                };
                (label, p.id.to_string())
            })
            .collect();
        (
            "Pick which permit to assign.".to_string(),
            vec![
                string_select(
                    cid(&["permit", "pick", kind_str(kind)]),
                    "Pick a permit…",
                    options,
                ),
                back,
            ],
        )
    };

    let opts = CardOptions {
        breadcrumbs: vec![
            "Hub".to_string(),
            "Permissions".to_string(),
            format!("Pick {kind_label} Permit"),
        ],
        separator_above_action_rows: true,
        action_rows,
        ..Default::default()
    };

    Ok(Card::lines("🛡️ Pick a Permit", vec![body])
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

/// Pick the role/user a permit is assigned to.
fn assign_target_card(permit_id: i32, permit_name: &str, kind: PermitKind) -> CardReply {
    let token = permit_id.to_string();
    let custom_id = cid(&["permit", "assign", token.as_str()]);
    let (select, body) = if kind == PermitKind::Enforced {
        (
            user_select(custom_id, "Pick a member…"),
            "Pick the member to assign this permit to.",
        )
    } else {
        (
            role_select(custom_id, "Pick a role…"),
            "Pick the role to assign this permit to.",
        )
    };

    let opts = CardOptions {
        breadcrumbs: vec![
            "Hub".to_string(),
            "Permissions".to_string(),
            permit_name.to_string(),
            "Pick Target".to_string(),
        ],
        separator_above_action_rows: true,
        action_rows: vec![select, back_button(cid(&["tab", "permissions"]))],
        ..Default::default()
    };

    Card::lines(format!("🛡️ {permit_name}"), vec![body.to_string()])
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply()
}

/// Pick a subject and permit node to explain.
async fn explain_picker_card(
    services: &Services,
    guild: &GuildCtx,
    subject: Option<u64>,
    node_page: usize,
) -> Result<CardReply, Error> {
    let names = node_names(services, guild).await?;
    let total = page_count(names.len(), EXPLAIN_NODES_PER_PAGE);
    let safe_page = node_page.min(total - 1);
    let start = safe_page
        .saturating_mul(EXPLAIN_NODES_PER_PAGE)
        .min(names.len());
    let end = start
        .saturating_add(EXPLAIN_NODES_PER_PAGE)
        .min(names.len());

    let mut action_rows = vec![user_select(cid(&["permexplainuser"]), "Pick a member…")];
    if let Some(user_id) = subject {
        let user_token = user_id.to_string();
        let shown = &names[start..end];
        if !shown.is_empty() {
            let options = shown.iter().map(|n| (n.clone(), n.clone())).collect();
            action_rows.push(string_select(
                cid(&["permexplainnode", user_token.as_str()]),
                "Pick a permit…",
                options,
            ));
        }
        if let Some(row) = pagination_row(
            &cid(&["permexplainnodepage", user_token.as_str()]),
            safe_page,
            total,
        ) {
            action_rows.push(row);
        }
    }
    action_rows.push(back_button(cid(&["tab", "permissions"])));

    let body = subject.map_or_else(
        || "-# Pick a member first, then a permit.".to_string(),
        |user_id| {
            format!("Subject: <@{user_id}>\n-# Pick a permit to see every tier that decides it.")
        },
    );

    let opts = CardOptions {
        breadcrumbs: vec![
            "Hub".to_string(),
            "Permissions".to_string(),
            "Explain".to_string(),
        ],
        separator_above_action_rows: true,
        action_rows,
        ..Default::default()
    };

    Ok(Card::lines("🔍 Explain Access", vec![body])
        .kind(CardKind::Primary)
        .options(opts)
        .into_reply())
}

/// The tier-by-tier walk for one subject and node.
async fn explain_card(
    services: &Services,
    guild: &GuildCtx,
    node: &str,
    subject: u64,
) -> Result<CardReply, Error> {
    let explain = ExplainSubject {
        user_id: subject,
        channel_id: None,
        role_ids: Vec::new(),
        is_owner: services.cfg.is_owner(subject),
        is_guild_owner: false,
        is_manager: false,
    };
    let rows = explain_permit(&services.db, guild.id, node, &explain).await?;

    let lines: Vec<String> = rows
        .iter()
        .map(|row| {
            let mark = match row.verdict {
                Verdict::Allow => "✅",
                Verdict::Deny => "⛔",
                Verdict::Pass => "➖",
            };
            let word = match row.verdict {
                Verdict::Allow => "allow",
                Verdict::Deny => "deny",
                Verdict::Pass => "pass",
            };
            let detail = row
                .detail
                .as_ref()
                .map_or(String::new(), |d| format!(" ({d})"));
            format!("-# {mark} `{}` → {word}{detail}", row.tier)
        })
        .collect();
    let denied = rows.iter().any(|row| row.verdict == Verdict::Deny);

    let body = if lines.is_empty() {
        vec!["-# No tiers evaluated.".to_string()]
    } else {
        lines
    };

    let opts = CardOptions {
        breadcrumbs: vec![
            "Hub".to_string(),
            "Permissions".to_string(),
            "Explain".to_string(),
        ],
        separator_above_action_rows: true,
        action_rows: vec![back_button(cid(&["permexplain"]))],
        ..Default::default()
    };

    Ok(Card::lines(
        format!(
            "🛡️ Why <@{subject}> {} use `{node}`",
            if denied { "can't" } else { "can" }
        ),
        body,
    )
    .kind(CardKind::Primary)
    .options(opts)
    .into_reply())
}

/// Every permit node known to the guild, unique and sorted.
async fn node_names(services: &Services, guild: &GuildCtx) -> Result<Vec<String>, Error> {
    let permits = lumi_db::list_permits(&services.db, guild.id).await?;
    let mut names: Vec<String> = permits.into_iter().flat_map(|p| p.nodes).collect();
    names.sort();
    names.dedup();
    Ok(names)
}

/// Route permission interactions.
pub async fn dispatch(
    services: &Services,
    guild: &GuildCtx,
    id: &HubId,
    data: &serenity::ComponentInteractionData,
    user_id: u64,
) -> Result<HubOutcome, Error> {
    match id.head().unwrap_or_default() {
        "permdel" | "permpage" | "permorder" | "permorderpage" | "permmove" => {
            dispatch_list(services, guild, id, user_id).await
        }
        "permexplain" | "permexplainnodepage" | "permexplainuser" | "permexplainnode" => {
            dispatch_explain(services, guild, id, data).await
        }
        "permit" => dispatch_permit(services, guild, id, data, user_id).await,
        _ => Ok(HubOutcome::Done),
    }
}

async fn dispatch_list(
    services: &Services,
    guild: &GuildCtx,
    id: &HubId,
    user_id: u64,
) -> Result<HubOutcome, Error> {
    match id.head().unwrap_or_default() {
        "permdel" => {
            if let Some((permit_id, target_type, target_str_id)) = id.get(1).and_then(parse_target)
                && let Ok(target_snowflake) = target_str_id.parse::<lumi_db::ids::SnowflakeId>()
            {
                lumi_db::unassign_permit(
                    &services.db,
                    guild.id,
                    permit_id,
                    target_type,
                    target_snowflake,
                )
                .await?;
                services
                    .permits
                    .reload_guild(&services.db, guild.id)
                    .await?;
            }
            Ok(HubOutcome::Update(
                list_card(services, guild, 0, user_id).await?,
            ))
        }
        "permpage" => Ok(HubOutcome::Update(
            list_card(services, guild, parse_page(id.get(1)), user_id).await?,
        )),
        "permorder" | "permorderpage" => Ok(HubOutcome::Update(
            order_card(services, guild, parse_page(id.get(1))).await?,
        )),
        "permmove" => {
            let moved = id.get(1).and_then(|payload| {
                let (permit_id, direction) = payload.split_once('|')?;
                Some((permit_id.parse::<i32>().ok()?, direction))
            });
            let page = if let Some((permit_id, direction)) = moved {
                lumi_db::move_permit(&services.db, guild.id, permit_id, direction).await?;
                services
                    .permits
                    .reload_guild(&services.db, guild.id)
                    .await?;
                let permits = lumi_db::list_permits(&services.db, guild.id).await?;
                permits
                    .iter()
                    .position(|p| p.id == permit_id)
                    .map_or(0, |i| i / PERMIT_ORDER_PER_PAGE)
            } else {
                0
            };
            Ok(HubOutcome::Update(order_card(services, guild, page).await?))
        }
        _ => Ok(HubOutcome::Done),
    }
}

async fn dispatch_explain(
    services: &Services,
    guild: &GuildCtx,
    id: &HubId,
    data: &serenity::ComponentInteractionData,
) -> Result<HubOutcome, Error> {
    match id.head().unwrap_or_default() {
        "permexplain" => Ok(HubOutcome::Update(
            explain_picker_card(services, guild, None, 0).await?,
        )),
        "permexplainnodepage" => {
            let subject = id.get(1).and_then(|s| s.parse::<u64>().ok());
            let page = id
                .segments()
                .iter()
                .rev()
                .find_map(|token| token.parse::<usize>().ok())
                .unwrap_or(0);
            Ok(HubOutcome::Update(
                explain_picker_card(services, guild, subject, page).await?,
            ))
        }
        "permexplainuser" => {
            let subject = first_value(data).and_then(|v| v.parse::<u64>().ok());
            Ok(HubOutcome::Update(
                explain_picker_card(services, guild, subject, 0).await?,
            ))
        }
        "permexplainnode" => {
            let subject = id.get(1).and_then(|s| s.parse::<u64>().ok());
            match (subject, first_value(data)) {
                (Some(subject), Some(node)) => Ok(HubOutcome::Update(
                    explain_card(services, guild, &node, subject).await?,
                )),
                _ => Ok(HubOutcome::Update(
                    explain_picker_card(services, guild, subject, 0).await?,
                )),
            }
        }
        _ => Ok(HubOutcome::Done),
    }
}

async fn dispatch_permit(
    services: &Services,
    guild: &GuildCtx,
    id: &HubId,
    data: &serenity::ComponentInteractionData,
    user_id: u64,
) -> Result<HubOutcome, Error> {
    match id.get(1) {
        Some("grant") => {
            let kind = parse_kind(&id.tail(2));
            Ok(HubOutcome::Update(
                picker_card(services, guild, kind).await?,
            ))
        }
        Some("pick") => {
            let kind = parse_kind(&id.tail(2));
            let permit = match first_value(data).and_then(|v| v.parse::<i32>().ok()) {
                Some(permit_id) => lumi_db::get_permit(&services.db, guild.id, permit_id).await?,
                None => None,
            };
            match permit {
                Some(permit) => Ok(HubOutcome::Update(assign_target_card(
                    permit.id,
                    &permit.name,
                    kind,
                ))),
                None => Ok(HubOutcome::Update(
                    list_card(services, guild, 0, user_id).await?,
                )),
            }
        }
        Some("assign") => {
            let target_type =
                if matches!(&data.kind, ComponentInteractionDataKind::RoleSelect { .. }) {
                    PermitTargetType::Role
                } else {
                    PermitTargetType::User
                };
            if let (Some(permit_id), Some(target_val)) =
                (id.tail(2).parse::<i32>().ok(), first_value(data))
                && let Ok(target_snowflake) = target_val.parse::<lumi_db::ids::SnowflakeId>()
            {
                lumi_db::assign_permit(
                    &services.db,
                    guild.id,
                    permit_id,
                    target_type,
                    target_snowflake,
                )
                .await?;
                services
                    .permits
                    .reload_guild(&services.db, guild.id)
                    .await?;
            }
            Ok(HubOutcome::Update(
                list_card(services, guild, 0, user_id).await?,
            ))
        }
        _ => Ok(HubOutcome::Done),
    }
}

fn page_count(len: usize, per: usize) -> usize {
    len.div_ceil(per).max(1)
}

const fn kind_str(kind: PermitKind) -> &'static str {
    match kind {
        PermitKind::Custom => "custom",
        PermitKind::Enforced => "enforced",
    }
}

fn parse_kind(token: &str) -> PermitKind {
    if token == "enforced" {
        PermitKind::Enforced
    } else {
        PermitKind::Custom
    }
}

const fn target_str(target_type: PermitTargetType) -> &'static str {
    match target_type {
        PermitTargetType::User => "user",
        PermitTargetType::Role => "role",
        PermitTargetType::Channel => "channel",
    }
}

const fn icon(kind: PermitKind) -> &'static str {
    match kind {
        PermitKind::Custom => "✅",
        PermitKind::Enforced => "🛡️",
    }
}

fn mention(target_type: PermitTargetType, target_id: impl std::fmt::Display) -> String {
    match target_type {
        PermitTargetType::Role => format!("<@&{target_id}>"),
        PermitTargetType::Channel => format!("<#{target_id}>"),
        PermitTargetType::User => format!("<@{target_id}>"),
    }
}

/// `permitId|targetType|targetId` from a `permdel` custom id.
fn parse_target(payload: &str) -> Option<(i32, PermitTargetType, &str)> {
    let mut parts = payload.split('|');
    let permit_id = parts.next()?.parse().ok()?;
    let target_type = match parts.next()? {
        "role" => PermitTargetType::Role,
        "user" => PermitTargetType::User,
        "channel" => PermitTargetType::Channel,
        _ => return None,
    };
    Some((permit_id, target_type, parts.next()?))
}

fn parse_page(token: Option<&str>) -> usize {
    token.and_then(|t| t.parse().ok()).unwrap_or(0)
}

fn first_value(data: &serenity::ComponentInteractionData) -> Option<String> {
    match &data.kind {
        ComponentInteractionDataKind::StringSelect { values } => values.first().cloned(),
        ComponentInteractionDataKind::UserSelect { values } => {
            values.first().map(|v| v.get().to_string())
        }
        ComponentInteractionDataKind::RoleSelect { values } => {
            values.first().map(|v| v.get().to_string())
        }
        _ => None,
    }
}
