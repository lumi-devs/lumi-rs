//! Moderation commands, grouped by concern.

mod cases;
mod channels;
mod punish;
mod utility;

use std::time::Duration;

use poise::serenity_prelude as serenity;
use serenity::all::{GuildId, UserId};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::Card;

pub(super) const MAX_TARGETS: usize = 25;

pub(super) async fn autocomplete_reason<'a>(
    _ctx: Context<'a>,
    partial: &'a str,
) -> serenity::CreateAutocompleteResponse<'a> {
    const PRESETS: &[&str] = &[
        "⚠️ Ban evasion",
        "🤖 Compromised account",
        "🚫 NSFW content",
        "🎯 Raiding",
        "🔗 Scam links",
        "⛔ Self-botting",
        "📧 Spam",
        "💬 Toxicity/Harassment",
    ];
    let needle = partial.to_lowercase();
    let choices: Vec<serenity::AutocompleteChoice> = PRESETS
        .iter()
        .filter(|reason| reason.to_lowercase().contains(&needle))
        .map(|reason| serenity::AutocompleteChoice::new(*reason, *reason))
        .collect();

    serenity::CreateAutocompleteResponse::new().set_choices(choices)
}

#[must_use]
pub(super) fn parse_target_list(single: Option<UserId>, extra: Option<&str>) -> Vec<UserId> {
    let mut targets = Vec::new();
    if let Some(single) = single {
        targets.push(single);
    }
    let Some(extra) = extra else {
        return targets;
    };

    static RE: std::sync::OnceLock<Option<regex::Regex>> = std::sync::OnceLock::new();
    let Some(regex) = crate::regex::cached(&RE, crate::patterns::TARGET_PATTERN) else {
        return targets;
    };

    for captures in regex.captures_iter(extra) {
        let Some(id) = captures
            .get(1)
            .or_else(|| captures.get(2))
            .and_then(|matched| matched.as_str().parse::<u64>().ok())
        else {
            continue;
        };
        let target = UserId::new(id);
        if !targets.contains(&target) {
            targets.push(target);
            if targets.len() == MAX_TARGETS {
                break;
            }
        }
    }
    targets
}

pub(super) fn guild_id(ctx: Context<'_>) -> Result<GuildId, Error> {
    ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))
}

pub(super) async fn send_card(ctx: Context<'_>, card: Card) -> Result<(), Error> {
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

pub(super) fn parse_duration(input: &str) -> Option<Duration> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    let split = input
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(input.len());
    let (number, unit) = input.split_at(split);
    let amount: u64 = number.parse().ok()?;
    let seconds = match unit.trim().to_lowercase().as_str() {
        "s" | "sec" | "secs" => amount,
        "m" | "min" | "mins" => amount.checked_mul(60)?,
        "h" | "hr" | "hrs" | "hour" | "hours" => amount.checked_mul(3_600)?,
        "d" | "day" | "days" => amount.checked_mul(86_400)?,
        "w" | "week" | "weeks" => amount.checked_mul(604_800)?,
        _ => return None,
    };
    Some(Duration::from_secs(seconds))
}

pub use cases::{case, cases, cases_delete, cases_modify, cases_view, note, reason};
pub use channels::{lock, lock_disable, lock_enable, lockdown, lockdown_disable, lockdown_enable};
pub use punish::{
    ban, kick, mute, quarantine, quarantine_add, quarantine_remove, softban, unban, unmute, warn,
};
pub use utility::{dm, sanitize, say};

#[cfg(test)]
mod tests {
    use super::{parse_duration, parse_target_list};
    use poise::serenity_prelude::UserId;
    use std::time::Duration;

    #[test]
    fn parses_durations_and_rejects_overflow() {
        assert_eq!(parse_duration("10m"), Some(Duration::from_secs(600)));
        assert_eq!(parse_duration("2 HOURS"), Some(Duration::from_secs(7_200)));
        assert_eq!(parse_duration("0d"), Some(Duration::ZERO));
        assert_eq!(parse_duration("10"), None);
        assert_eq!(parse_duration("10y"), None);
        assert_eq!(parse_duration("18446744073709551615w"), None);
    }

    #[test]
    fn target_list_deduplicates_and_caps() {
        let first = UserId::new(123_456_789_012_345_678);
        let list = parse_target_list(
            Some(first),
            Some(&format!(
                "<@{}> {}",
                first.get(),
                987_654_321_098_765_432_u64
            )),
        );
        assert_eq!(list.len(), 2);
        assert_eq!(list[0], first);
    }
}

pub fn commands() -> Vec<Command> {
    vec![
        warn(),
        kick(),
        ban(),
        unban(),
        mute(),
        unmute(),
        cases(),
        case(),
        reason(),
        note(),
        softban(),
        lock(),
        lockdown(),
        quarantine(),
        dm(),
        say(),
        sanitize(),
    ]
}
