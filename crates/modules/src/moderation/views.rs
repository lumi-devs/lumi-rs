use serenity::all::Colour;

use lumi_db::entities::{enums::CaseAction, mod_note, moderation_case};
use lumi_ui::{Card, CardOptions, CardReply};

pub fn action_color(action: &CaseAction) -> Colour {
    match action {
        CaseAction::Warn => Colour::new(0x00f5_9f00),     // Orange
        CaseAction::Mute | CaseAction::VoiceMute => Colour::new(0x00fab005), // Yellow
        CaseAction::Kick => Colour::new(0x00fa_5252),     // Red
        CaseAction::Ban | CaseAction::Softban => Colour::new(0x00c9_2a2a), // Dark Red
        CaseAction::Unban | CaseAction::Unmute | CaseAction::UnvoiceMute => Colour::new(0x0012_b886), // Green
        _ => Colour::new(0x004c_6ef5),
    }
}

pub fn action_emoji(action: &CaseAction) -> &'static str {
    match action {
        CaseAction::Warn => "⚠️",
        CaseAction::Mute | CaseAction::VoiceMute => "🔇",
        CaseAction::Kick => "👢",
        CaseAction::Ban | CaseAction::Softban => "🔨",
        CaseAction::Unban | CaseAction::Unmute | CaseAction::UnvoiceMute => "✅",
        _ => "🛡️",
    }
}

pub fn case_card(case: &moderation_case::Model) -> CardReply {
    let action_str = format!("{:?}", case.action);
    let title = format!(
        "{} Case #{} | {}",
        action_emoji(&case.action),
        case.case_number,
        action_str.to_uppercase()
    );

    let mut lines = Vec::new();
    lines.push(format!("**User:** <@{}> (`{}`)", case.user_id, case.user_id));
    lines.push(format!("**Moderator:** <@{}> (`{}`)", case.moderator_id, case.moderator_id));
    
    if let Some(dur) = case.duration {
        lines.push(format!("**Duration:** {} seconds", dur));
    }

    let reason = case.reason.as_deref().unwrap_or("No reason provided.");
    lines.push(format!("**Reason:** {}", reason));

    Card::lines(title, lines)
        .accent(Some(action_color(&case.action)))
        .into_reply()
}

pub fn mod_batch_success_card(
    action: &CaseAction,
    target_ids: &[u64],
    case_nums: &[i32],
    reason: Option<&str>,
) -> CardReply {
    let action_name = format!("{:?}", action);
    let title = format!("{} Batch {}", action_emoji(action), action_name);

    let mut lines = Vec::new();
    let targets = target_ids
        .iter()
        .map(|id| format!("<@{id}>"))
        .collect::<Vec<_>>()
        .join(", ");
    let cases = case_nums
        .iter()
        .map(|num| format!("`#{num}`"))
        .collect::<Vec<_>>()
        .join(", ");

    lines.push(format!("Successfully applied **{}** to {}.", action_name.to_lowercase(), targets));
    lines.push(format!("**Cases:** {cases}"));
    if let Some(r) = reason {
        lines.push(format!("**Reason:** {}", r));
    }

    Card::lines(title, lines)
        .accent(Some(action_color(action)))
        .into_reply()
}

pub fn mod_action_success_card(
    action: &CaseAction,
    target_id: u64,
    case_num: i32,
    reason: Option<&str>,
) -> CardReply {
    let action_name = format!("{:?}", action);
    let title = format!("{} {}", action_emoji(action), action_name);

    let mut lines = Vec::new();
    lines.push(format!("Successfully applied **{}** to <@{}>.", action_name.to_lowercase(), target_id));
    lines.push(format!("**Case:** `#{}`", case_num));
    if let Some(r) = reason {
        lines.push(format!("**Reason:** {}", r));
    }

    Card::lines(title, lines)
        .accent(Some(action_color(action)))
        .into_reply()
}

pub fn notes_list_card(user_id: u64, notes: &[mod_note::Model]) -> CardReply {
    let title = format!("📝 Notes for <@{}>", user_id);

    if notes.is_empty() {
        return Card::lines(title, vec!["No notes found for this user.".to_string()]).into_reply();
    }

    let mut lines = Vec::new();
    for (i, note) in notes.iter().enumerate() {
        lines.push(format!(
            "**{}.** {} (by <@{}> at <t:{}:d>)",
            i + 1,
            note.message,
            note.author_id,
            note.created_at.timestamp()
        ));
    }

    let mut opts = CardOptions::default();
    opts.footer = Some(format!("Total: {} note(s)", notes.len()));

    Card::lines(title, lines)
        .options(opts)
        .into_reply()
}
