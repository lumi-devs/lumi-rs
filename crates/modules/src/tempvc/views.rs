use serenity::all::{ChannelType, CreateActionRow};
use lumi_ui::panels::{action_row_of, channel_select_of_types, string_select_options};
use lumi_ui::{Card, CardKind, CardOptions, CardReply};

/// One configured generator, resolved for display.
#[derive(Debug, Clone)]
pub struct GeneratorView {
    pub channel_id: u64,
    pub channel_name: String,
    pub template: String,
    pub limit: i32,
}

pub const PREFIX: &str = "tempvc";

const MANAGE_OPTIONS: &[(&str, &str, &str)] = &[
    ("Rename Channel", "name", "\u{1f4dd}"),
    ("Set User Limit", "limit", "\u{1f465}"),
    ("Lock Channel", "lock", "\u{1f512}"),
    ("Hide Channel", "hide", "\u{1f575}\u{fe0f}"),
    ("Kick Members", "kick", "\u{1f462}"),
    ("Trust Member / Role", "trust", "\u{2705}"),
    ("Untrust Member / Role", "untrust", "\u{274e}"),
    ("Block Member / Role", "block", "\u{1f6ab}"),
    ("Unblock Member / Role", "unblock", "\u{267b}\u{fe0f}"),
    ("Transfer Ownership", "transfer", "\u{1f504}"),
    ("Delete Channel", "delete", "\u{1f5d1}\u{fe0f}"),
];

pub fn path(channel_id: u64) -> String { channel_id.to_string() }

pub fn menu_id(channel_id: u64) -> String {
    format!("{PREFIX}:menu:{}", path(channel_id))
}
pub fn claim_id(channel_id: u64) -> String {
    format!("{PREFIX}:claim:{}", path(channel_id))
}
pub fn back_id(channel_id: u64) -> String {
    format!("{PREFIX}:back:{}", path(channel_id))
}
pub fn pick_id(action: &str, channel_id: u64) -> String {
    format!("{PREFIX}:pick:{action}:{}", path(channel_id))
}
pub fn pick_role_id(action: &str, channel_id: u64) -> String {
    format!("{PREFIX}:pickrole:{action}:{}", path(channel_id))
}
pub fn delete_confirm_id(channel_id: u64) -> String {
    format!("{PREFIX}:delok:{}", path(channel_id))
}
pub fn rename_modal_id(channel_id: u64) -> String {
    format!("{PREFIX}:namem:{}", path(channel_id))
}
pub fn limit_modal_id(channel_id: u64) -> String {
    format!("{PREFIX}:limitm:{}", path(channel_id))
}

pub fn panel_card(
    channel_id: u64,
    owner_id: u64,
    locked: bool,
    hidden: bool,
    limit: i32,
) -> CardReply {
    let status = match (locked, hidden) {
        (true, true) => ("\u{1f512}", "Locked \u{b7} Hidden"),
        (true, false) => ("\u{1f512}", "Locked"),
        (false, true) => ("\u{1f575}\u{fe0f}", "Hidden"),
        (false, false) => ("\u{1f513}", "Open"),
    };
    let limit = if limit > 0 {
        limit.to_string()
    } else {
        "Unlimited".to_string()
    };

    let mut options: Vec<(String, String, Option<&str>)> = Vec::new();
    for (label, action, emoji) in MANAGE_OPTIONS {
        let (label, emoji) = match *action {
            "lock" if locked => ("Unlock Channel", "\u{1f513}"),
            "hide" if hidden => ("Unhide Channel", "\u{1f440}"),
            _ => (*label, *emoji),
        };
        options.push((label.to_string(), (*action).to_string(), Some(emoji)));
    }

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec!["Temp VC".to_string(), "Channel".to_string()];
    opts.status_badge = Some((status.0.to_string(), Some(status.1.to_string())));
    opts.footer = Some(
        "Only the channel owner (or staff) can use these controls \u{b7} claim it if the owner left."
            .to_string(),
    );
    opts.separator_above_action_rows = true;
    opts.action_rows = vec![
        string_select_options(menu_id(channel_id), "Manage Channel\u{2026}", options),
        action_row_of(vec![claim_button(channel_id)]),
    ];

    Card::lines(
        "\u{1f399}\u{fe0f} Voice Channel Control Deck",
        vec![
            format!("**Channel:** <#{channel_id}>"),
            format!(
                "**Owner:** <@{owner_id}> \u{b7} **Limit:** {limit}"
            ),
            format!("**Status:** {} **{}**", status.0, status.1),
        ],
    )
    .kind(CardKind::Primary)
    .options(opts)
    .ephemeral()
    .into_reply()
}

fn claim_button(channel_id: u64) -> serenity::all::CreateButton<'static> {
    let mut button = serenity::all::CreateButton::new(claim_id(channel_id))
        .label("\u{1f3af} Claim Ownership")
        .style(serenity::all::ButtonStyle::Primary);
    if let Some(emoji) = lumi_ui::panels::component_emoji("\u{1f3af}") {
        button = button.emoji(emoji);
    }
    button
}

pub fn picker_card(action: &str, channel_id: u64) -> CardReply {
    let (title, body) = match action {
        "trust" => (
            "\u{2705} Trust Member or Role",
            "Pick who may connect, view and speak in your channel.",
        ),
        "untrust" => (
            "\u{274e} Untrust Member or Role",
            "Pick whose custom permissions should be removed.",
        ),
        "block" => (
            "\u{1f6ab} Block Member or Role",
            "Pick who should not be able to see or join your channel.",
        ),
        "unblock" => (
            "\u{267b}\u{fe0f} Unblock Member or Role",
            "Pick whose block should be lifted.",
        ),
        "kick" => (
            "\u{1f462} Kick Members",
            "Pick members to disconnect from your channel.",
        ),
        "transfer" => (
            "\u{1f504} Transfer Ownership",
            "Pick the member who should own this channel.",
        ),
        _ => ("TempVC", "Pick a member."),
    };

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec!["Temp VC".to_string(), title.to_string()];
    opts.separator_above_action_rows = true;
    opts.footer = Some("Pick a member or role, then press \u{2190} Back to return.".to_string());

    let mut rows: Vec<CreateActionRow<'static>> = Vec::new();
    let member_placeholder = match action {
        "kick" => "Select member(s) to kick\u{2026}",
        "transfer" => "Select the new owner\u{2026}",
        _ => "Select member(s)\u{2026}",
    };
    rows.push(user_select_multi(pick_id(action, channel_id), member_placeholder, action != "transfer"));
    if matches!(action, "trust" | "untrust" | "block" | "unblock") {
        rows.push(role_select_multi(
            pick_role_id(action, channel_id),
            "Select role(s) (optional)\u{2026}",
        ));
    }
    rows.push(action_row_of(vec![back_button(channel_id)]));

    opts.action_rows = rows;
    Card::new(title, body)
        .kind(CardKind::Info)
        .options(opts)
        .ephemeral()
        .into_reply()
}

fn user_select_multi(
    custom_id: String,
    placeholder: impl Into<String>,
    multi: bool,
) -> CreateActionRow<'static> {
    let mut menu = serenity::all::CreateSelectMenu::new(
        custom_id,
        serenity::all::CreateSelectMenuKind::User {
            default_users: None,
        },
    )
    .placeholder(placeholder.into());
    if multi {
        menu = menu.max_values(10);
    }
    CreateActionRow::SelectMenu(menu)
}

fn role_select_multi(custom_id: String, placeholder: impl Into<String>) -> CreateActionRow<'static> {
    CreateActionRow::SelectMenu(
        serenity::all::CreateSelectMenu::new(
            custom_id,
            serenity::all::CreateSelectMenuKind::Role {
                default_roles: None,
            },
        )
        .placeholder(placeholder.into())
        .max_values(10),
    )
}

fn back_button(channel_id: u64) -> serenity::all::CreateButton<'static> {
    serenity::all::CreateButton::new(back_id(channel_id))
        .label("\u{2190} Back to Panel")
        .style(serenity::all::ButtonStyle::Secondary)
}

pub fn delete_confirm_card(channel_id: u64) -> CardReply {
    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec!["Temp VC".to_string(), "Delete".to_string()];
    opts.separator_above_action_rows = true;
    opts.action_rows = vec![action_row_of(vec![
        serenity::all::CreateButton::new(delete_confirm_id(channel_id))
            .label("Confirm Delete")
            .style(serenity::all::ButtonStyle::Danger),
        back_button(channel_id),
    ])];
    Card::new(
        "\u{1f5d1}\u{fe0f} Delete Channel?",
        "This permanently deletes your voice channel for everyone.",
    )
    .kind(CardKind::Error)
    .options(opts)
    .ephemeral()
    .into_reply()
}

pub fn admin_card(generators: &[GeneratorView]) -> CardReply {
    let body = if generators.is_empty() {
        "-# *No generators yet. Pick a voice channel below to add one.*".to_string()
    } else {
        generators
            .iter()
            .map(|g| {
                let limit = if g.limit > 0 {
                    format!("limit {}", g.limit)
                } else {
                    "no limit".to_string()
                };
                format!("**{}** \u{b7} `{}` \u{b7} {limit}", g.channel_name, g.template)
            })
            .collect::<Vec<_>>()
            .join("\n")
    };

    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec!["Temp VC".to_string(), "Generators".to_string()];
    opts.footer = Some(if generators.is_empty() {
        "Pick a voice channel below to add your first generator.".to_string()
    } else {
        format!(
            "{} generators \u{b7} pick one below to edit it",
            generators.len()
        )
    });
    opts.separator_above_action_rows = true;

    let mut rows = vec![channel_select_of_types(
        admin_add_id(),
        "Add a generator channel\u{2026}",
        vec![ChannelType::Voice],
    )];
    if !generators.is_empty() {
        let options = generators
            .iter()
            .take(25)
            .map(|g| (g.channel_name.clone(), g.channel_id.to_string(), None))
            .collect();
        rows.push(string_select_options(
            admin_manage_id(),
            "Manage a generator\u{2026}",
            options,
        ));
    }
    opts.action_rows = rows;

    Card::new("\u{1f50a} Temp VC Generators", body)
        .kind(CardKind::Info)
        .options(opts)
        .into_reply()
}

pub fn generator_card(generator: &GeneratorView) -> CardReply {
    let limit = if generator.limit > 0 {
        format!("{} users", generator.limit)
    } else {
        "unlimited".to_string()
    };
    let mut opts = CardOptions::default();
    opts.breadcrumbs = vec![
        "Temp VC".to_string(),
        "Generators".to_string(),
        generator.channel_name.clone(),
    ];
    opts.footer = Some(
        "Rename edits the template ({username}, {number}, {pos}) \u{b7} Set Limit caps users \u{b7} 0 = unlimited."
            .to_string(),
    );
    opts.separator_above_action_rows = true;
    opts.action_rows = vec![action_row_of(vec![
        serenity::all::CreateButton::new(admin_rename_id(generator.channel_id))
            .label("Rename")
            .style(serenity::all::ButtonStyle::Secondary),
        serenity::all::CreateButton::new(admin_limit_id(generator.channel_id))
            .label("Set Limit")
            .style(serenity::all::ButtonStyle::Secondary),
        serenity::all::CreateButton::new(admin_remove_id(generator.channel_id))
            .label("Remove")
            .style(serenity::all::ButtonStyle::Danger),
        serenity::all::CreateButton::new(admin_back_id())
            .label("Back")
            .style(serenity::all::ButtonStyle::Secondary),
    ])];

    Card::lines(
        format!("\u{1f50a} {}", generator.channel_name),
        vec![
            format!("**Channel:** <#{}>", generator.channel_id),
            format!("**Name template:** `{}`", generator.template),
            format!("**User limit:** {limit}"),
        ],
    )
    .kind(CardKind::Info)
    .options(opts)
    .into_reply()
}

pub fn admin_add_id() -> String {
    format!("{PREFIX}:admin:add")
}
pub fn admin_manage_id() -> String {
    format!("{PREFIX}:admin:manage")
}
pub fn admin_back_id() -> String {
    format!("{PREFIX}:admin:back")
}
pub fn admin_rename_id(channel_id: u64) -> String {
    format!("{PREFIX}:admin:name:{channel_id}")
}
pub fn admin_limit_id(channel_id: u64) -> String {
    format!("{PREFIX}:admin:limit:{channel_id}")
}
pub fn admin_remove_id(channel_id: u64) -> String {
    format!("{PREFIX}:admin:rm:{channel_id}")
}
pub fn admin_name_modal_id(channel_id: u64) -> String {
    format!("{PREFIX}:admin:namem:{channel_id}")
}
pub fn admin_limit_modal_id(channel_id: u64) -> String {
    format!("{PREFIX}:admin:limitm:{channel_id}")
}

pub fn generator_setup_card(channel_id: u64, name: &str, limit: i32) -> CardReply {
    let mut lines = Vec::new();
    lines.push(format!("**Generator Channel:** <#{channel_id}>"));
    lines.push(format!("**Template Name:** `{name}`"));
    if limit > 0 {
        lines.push(format!("**User Limit:** `{limit}`"));
    } else {
        lines.push("**User Limit:** `Unlimited`".to_string());
    }

    Card::lines("Temporary VC Generator Configured", lines)
        .kind(CardKind::Success)
        .into_reply()
}

pub fn generator_deleted_card(channel_id: u64) -> CardReply {
    Card::success(
        "Temporary VC Generator Removed",
        format!("Successfully removed generator configuration for <#{channel_id}>."),
    )
    .into_reply()
}

pub fn error_card(msg: &str) -> CardReply {
    Card::error("Temporary VC Error", msg).into_reply()
}
