use crate::ui::{Card, CardOptions, CardReply};

/// Card shown when a message mentions an AFK user (thumbnail = their avatar).
pub fn mention_card(
    name: &str,
    reason: &str,
    since: &str,
    avatar_url: Option<String>,
) -> CardReply {
    let options = CardOptions {
        thumbnail_url: avatar_url,
        footer: Some(format!("AFK since {since}")),
        ..CardOptions::default()
    };
    Card::lines(format!("\u{1f4a4} {name} is AFK"), vec![reason.to_string()])
        .options(options)
        .into_reply()
}

pub fn welcome_back_card(name: &str) -> CardReply {
    Card::success(
        "Welcome back",
        format!("**{name}**, your AFK status has been cleared."),
    )
    .into_reply()
}
