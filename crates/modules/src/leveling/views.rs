use lumi_db::entities::level_account;
use lumi_ui::{Card, CardKind, CardOptions, CardReply};

#[must_use]
pub fn rank_card(
    username: &str,
    avatar_url: Option<String>,
    rank: Option<i64>,
    next_level_xp: i64,
    account: &level_account::Model,
) -> CardReply {
    let rank_str = rank.map_or_else(|| "Unranked".to_string(), |r| format!("#{r}"));
    let level = account.level;
    let current_xp = account.xp;
    let total_xp = account.total_xp;
    let message_count = account.message_count;
    let voice_minutes = account.voice_minutes;
    let lines = vec![
        format!("**Rank**: `{rank_str}` \u{00b7} **Level**: `{level}`"),
        format!("**Progress**: `{current_xp}` / `{next_level_xp}` XP"),
        format!("**Total XP**: `{total_xp}`"),
        format!("**Messages**: `{message_count}` \u{00b7} **Voice**: `{voice_minutes}m`"),
    ];

    let options = CardOptions {
        thumbnail_url: avatar_url,
        footer: Some(format!(
            "{current_xp} / {next_level_xp} XP needed for next level"
        )),
        ..CardOptions::default()
    };

    Card::lines(format!("📊 Rank \u{2014} {username}"), lines)
        .kind(CardKind::Purple)
        .options(options)
        .into_reply()
}

#[must_use]
pub fn no_rank_card(username: &str) -> CardReply {
    Card::info(
        "No Rank Yet",
        format!("**{username}** has not earned any XP on this server yet."),
    )
    .into_reply()
}

#[must_use]
pub fn leaderboard_card(entries: &[level_account::Model]) -> CardReply {
    if entries.is_empty() {
        return Card::info(
            "🏆 Server Leaderboard",
            "Nobody has earned XP on this server yet.",
        )
        .into_reply();
    }

    let mut lines = Vec::new();
    for (idx, entry) in entries.iter().enumerate() {
        let rank = idx + 1;
        let medal = match rank {
            1 => "🥇",
            2 => "🥈",
            3 => "🥉",
            _ => "▫️",
        };
        lines.push(format!(
            "{medal} **#{rank}** <@{}> \u{2014} **Level {}** (`{}` XP)",
            entry.user_id, entry.level, entry.total_xp
        ));
    }

    let options = CardOptions {
        footer: Some(format!("Top {} members", entries.len())),
        ..CardOptions::default()
    };

    Card::lines("🏆 Server Leaderboard", lines)
        .kind(CardKind::Gold)
        .options(options)
        .into_reply()
}
