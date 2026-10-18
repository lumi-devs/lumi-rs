use lumi_ui::{Card, CardReply};

pub fn generator_setup_card(channel_id: u64, name: &str, limit: i32) -> CardReply {
    let mut lines = Vec::new();
    lines.push(format!("**Generator Channel:** <#{channel_id}>"));
    lines.push(format!("**Template Name:** `{name}`"));
    if limit > 0 {
        lines.push(format!("**User Limit:** `{limit}`"));
    } else {
        lines.push("**User Limit:** `Unlimited`".to_string());
    }

    Card::lines("Temporary VC Generator Configured", lines).into_reply()
}

pub fn generator_deleted_card(channel_id: u64) -> CardReply {
    Card::success(
        "Temporary VC Generator Removed",
        format!("Successfully removed generator configuration for <#{channel_id}>."),
    )
    .into_reply()
}

pub fn channel_renamed_card(name: &str) -> CardReply {
    Card::success("Voice Channel Renamed", format!("Channel name updated to: **{name}**"))
        .into_reply()
}

pub fn channel_limit_card(limit: i32) -> CardReply {
    let text = if limit > 0 {
        format!("Channel user limit set to **{limit}**.")
    } else {
        "Channel user limit removed.".to_string()
    };
    Card::success("User Limit Updated", text).into_reply()
}

pub fn channel_locked_card(locked: bool) -> CardReply {
    let text = if locked {
        "Channel is now **locked**."
    } else {
        "Channel is now **unlocked**."
    };
    Card::success("Channel Privacy Updated", text).into_reply()
}

pub fn error_card(msg: &str) -> CardReply {
    Card::error("Temporary VC Error", msg).into_reply()
}
