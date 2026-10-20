use serenity::all::{Colour, UserId};

use lumi_db::entities::{enums::CaseAction, mod_note, moderation_case};
use lumi_ui::{Card, CardOptions, CardReply};

use super::actions::{AppliedCase, BulkOutcome};
use crate::text::cut;

/// Accent colour per action. Colours are plain `0xRRGGBB` values: an extra
/// leading byte silently turns the intended hue into another one.
pub fn action_color(action: &CaseAction) -> Colour {
    match action {
        CaseAction::Warn => Colour::new(0xf5_9f00),
        CaseAction::Mute | CaseAction::VoiceMute => Colour::new(0xfab005),
        CaseAction::Kick => Colour::new(0xfa_5252),
        CaseAction::Ban | CaseAction::Softban => Colour::new(0xc9_2a2a),
        CaseAction::Unban
        | CaseAction::Unmute
        | CaseAction::UnvoiceMute
        | CaseAction::Unquarantine => Colour::new(0x12_b886),
        _ => Colour::new(0x4c_6ef5),
    }
}

pub fn action_emoji(action: &CaseAction) -> &'static str {
    match action {
        CaseAction::Warn => "⚠️",
        CaseAction::Mute | CaseAction::VoiceMute => "🔇",
        CaseAction::Kick => "👢",
        CaseAction::Ban | CaseAction::Softban => "🔨",
        CaseAction::Unban
        | CaseAction::Unmute
        | CaseAction::UnvoiceMute
        | CaseAction::Unquarantine => "✅",
        CaseAction::Quarantine => "🚧",
        CaseAction::AntinukeAlert => "🚨",
    }
}

/// Stable human label, replacing `format!("{action:?}")` scattered across cards.
pub fn action_label(action: &CaseAction) -> &'static str {
    match action {
        CaseAction::Ban => "ban",
        CaseAction::Unban => "unban",
        CaseAction::Kick => "kick",
        CaseAction::Mute => "mute",
        CaseAction::Unmute => "unmute",
        CaseAction::Softban => "softban",
        CaseAction::VoiceMute => "voice mute",
        CaseAction::UnvoiceMute => "voice unmute",
        CaseAction::Warn => "warn",
        CaseAction::Quarantine => "quarantine",
        CaseAction::Unquarantine => "unquarantine",
        CaseAction::AntinukeAlert => "anti-nuke alert",
    }
}

/// Parse the `/cases view action:` filter without a lossy fallback.
pub fn action_from_str(value: &str) -> Option<CaseAction> {
    match value.trim().to_lowercase().as_str() {
        "warn" => Some(CaseAction::Warn),
        "kick" => Some(CaseAction::Kick),
        "ban" => Some(CaseAction::Ban),
        "unban" => Some(CaseAction::Unban),
        "mute" | "timeout" => Some(CaseAction::Mute),
        "unmute" => Some(CaseAction::Unmute),
        "softban" => Some(CaseAction::Softban),
        "quarantine" => Some(CaseAction::Quarantine),
        "unquarantine" => Some(CaseAction::Unquarantine),
        "antinuke_alert" => Some(CaseAction::AntinukeAlert),
        _ => None,
    }
}

/// Largest fitting unit, e.g. `45s`, `10m`, `2h`, `7d`.
#[must_use]
pub fn human_seconds(seconds: i32) -> String {
    let seconds = u64::try_from(seconds).unwrap_or_default();
    if seconds < 60 {
        format!("{seconds}s")
    } else if seconds < 3_600 {
        format!("{}m", seconds / 60)
    } else if seconds < 86_400 {
        format!("{}h", seconds / 3_600)
    } else {
        format!("{}d", seconds / 86_400)
    }
}

pub fn case_card(case: &moderation_case::Model) -> CardReply {
    let title = format!(
        "{} Case #{} | {}",
        action_emoji(&case.action),
        case.case_number,
        action_label(&case.action).to_uppercase()
    );

    let mut lines = vec![
        format!("**User:** <@{}> (`{}`)", case.user_id, case.user_id),
        format!(
            "**Moderator:** <@{}> (`{}`)",
            case.moderator_id, case.moderator_id
        ),
    ];

    if let Some(duration) = case.duration {
        lines.push(format!(
            "**Duration:** {} ({duration}s)",
            human_seconds(duration)
        ));
    }

    let reason = case.reason.as_deref().unwrap_or("No reason provided.");
    lines.push(format!("**Reason:** {reason}"));

    Card::lines(title, lines)
        .accent(Some(action_color(&case.action)))
        .into_reply()
}

/// One card for a single or bulk punishment, including partial failures and
/// cases that could not be persisted.
pub fn punishment_card(outcome: &BulkOutcome, reason: Option<&str>) -> CardReply {
    let action = outcome.action;
    let label = action_label(&action);
    let title = format!(
        "{} {} {}",
        action_emoji(&action),
        label.to_uppercase(),
        if outcome.applied.len() > 1 {
            format!("×{}", outcome.applied.len())
        } else {
            String::new()
        }
    );

    let mut lines = Vec::new();
    if let Some(AppliedCase { target, case }) = outcome
        .applied
        .first()
        .filter(|_| outcome.applied.len() == 1)
    {
        lines.push(format!("Applied **{label}** to <@{}>.", target.get()));
        match case {
            Some(case) => lines.push(format!("**Case:** `#{}`", case.case_number)),
            None => lines.push("**Case:** not recorded (database error)".to_string()),
        }
    } else if !outcome.applied.is_empty() {
        let targets = outcome
            .applied
            .iter()
            .map(|applied| format!("<@{}>", applied.target.get()))
            .collect::<Vec<_>>()
            .join(", ");
        let cases = outcome
            .applied
            .iter()
            .map(|applied| {
                applied.case.as_ref().map_or_else(
                    || "`#?`".to_string(),
                    |case| format!("`#{}`", case.case_number),
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "Applied **{label}** to {} member(s).",
            outcome.applied.len()
        ));
        lines.push(format!("**Targets:** {targets}"));
        lines.push(format!("**Cases:** {cases}"));
    }

    if !outcome.failures.is_empty() {
        let failures = outcome
            .failures
            .iter()
            .take(10)
            .map(|(target, error)| format!("<@{}>: {}", target.get(), cut(error, 160)))
            .collect::<Vec<_>>()
            .join("\n");
        lines.push(format!("**Failed:**\n{failures}"));
    }

    if let Some(reason) = reason {
        lines.push(format!("**Reason:** {reason}"));
    }

    if outcome.applied.is_empty() && !outcome.failures.is_empty() {
        return Card::lines(format!("❌ {} Failed", label.to_uppercase()), lines)
            .kind(lumi_ui::CardKind::Error)
            .into_reply();
    }

    Card::lines(title.trim_end(), lines)
        .accent(Some(action_color(&action)))
        .into_reply()
}

/// Convenience wrapper for the single-target commands.
pub fn action_success_card(
    action: &CaseAction,
    target: u64,
    case: Option<moderation_case::Model>,
    reason: Option<&str>,
) -> CardReply {
    punishment_card(
        &BulkOutcome {
            action: *action,
            applied: vec![AppliedCase {
                target: UserId::new(target),
                case,
            }],
            failures: Vec::new(),
        },
        reason,
    )
}

pub fn notes_list_card(user_id: u64, notes: &[mod_note::Model]) -> CardReply {
    let title = format!("📝 Notes for <@{}>", user_id);

    if notes.is_empty() {
        return Card::lines(title, vec!["No notes found for this user.".to_string()]).into_reply();
    }

    let lines = notes
        .iter()
        .enumerate()
        .map(|(index, note)| {
            format!(
                "**{}.** {} (by <@{}> at <t:{}:d>)",
                index + 1,
                note.message,
                note.author_id,
                note.created_at.timestamp()
            )
        })
        .collect();

    let mut options = CardOptions::default();
    options.footer = Some(format!("Total: {} note(s)", notes.len()));

    Card::lines(title, lines).options(options).into_reply()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_filter_parses_aliases() {
        assert_eq!(action_from_str("TIMEOUT"), Some(CaseAction::Mute));
        assert_eq!(action_from_str("ban"), Some(CaseAction::Ban));
        assert_eq!(action_from_str("nope"), None);
    }

    #[test]
    fn durations_use_largest_unit() {
        assert_eq!(human_seconds(45), "45s");
        assert_eq!(human_seconds(600), "10m");
        assert_eq!(human_seconds(7_200), "2h");
        assert_eq!(human_seconds(172_800), "2d");
        assert_eq!(human_seconds(-1), "0s");
    }
}
