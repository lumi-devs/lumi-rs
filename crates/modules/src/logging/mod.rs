//! Logging module: server event logs (messages, members, bans) as V2 cards.

pub mod events;
pub mod store;
pub mod views;

use serde_json::json;

use lumi_kernel::module::{ConfigField, ConfigFieldKind, Module, ModuleMeta};

pub struct LoggingModule;

fn channel(key: &str, label: &str, description: &str, group: &str) -> ConfigField {
    ConfigField {
        key: key.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        kind: ConfigFieldKind::Channel,
        default: json!(null),
        subfields: Vec::new(),
        group: Some(group.to_string()),
    }
}

fn toggle(key: &str, label: &str, description: &str, group: &str) -> ConfigField {
    ConfigField {
        key: key.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        kind: ConfigFieldKind::Boolean,
        default: json!(true),
        subfields: Vec::new(),
        group: Some(group.to_string()),
    }
}

fn string_array(key: &str, label: &str, description: &str, group: &str) -> ConfigField {
    ConfigField {
        key: key.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        kind: ConfigFieldKind::StringArray,
        default: json!([]),
        subfields: Vec::new(),
        group: Some(group.to_string()),
    }
}

impl Module for LoggingModule {
    fn name(&self) -> &'static str {
        "logging"
    }

    fn metadata(&self) -> ModuleMeta {
        ModuleMeta {
            display_name: "Logging".to_string(),
            emoji: "\u{1f4cb}".to_string(),
            description: "Server event logs: message deletes/edits, joins, leaves, bans, unbans, nickname and role changes.".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        let setup = "Setup";
        let message = "Message Events";
        let member = "Member Events";
        vec![
            channel(
                "log_channel_id",
                "Default Log Channel",
                "Fallback channel for log types without their own channel. Empty disables logging.",
                setup,
            ),
            channel(
                "message_log_channel_id",
                "Message Log Channel",
                "Channel for message deletes and edits. Falls back to the default log channel.",
                message,
            ),
            string_array(
                "ignored_channels",
                "Ignored Channel IDs",
                "Channel IDs whose message events are not logged.",
                setup,
            ),
            channel(
                "member_log_channel_id",
                "Member Log Channel",
                "Channel for joins, leaves, bans, unbans, nickname and role changes. Falls back to the default log channel.",
                member,
            ),
            toggle("message_deletes", "Message Deletes", "Log deleted messages.", message),
            channel(
                "message_deletes_channel_id",
                "Message Deletes Channel",
                "Override for deleted messages.",
                message,
            ),
            toggle("message_edits", "Message Edits", "Log edited messages (before/after).", message),
            channel(
                "message_edits_channel_id",
                "Message Edits Channel",
                "Override for edited messages.",
                message,
            ),
            toggle("member_joins", "Member Joins", "Log members joining the server.", member),
            channel(
                "member_joins_channel_id",
                "Member Joins Channel",
                "Override for member joins.",
                member,
            ),
            toggle("member_leaves", "Member Leaves", "Log members leaving the server.", member),
            channel(
                "member_leaves_channel_id",
                "Member Leaves Channel",
                "Override for member leaves.",
                member,
            ),
            toggle("member_bans", "Member Bans", "Log bans.", member),
            channel(
                "member_bans_channel_id",
                "Member Bans Channel",
                "Override for bans.",
                member,
            ),
            toggle("member_unbans", "Member Unbans", "Log unbans.", member),
            channel(
                "member_unbans_channel_id",
                "Member Unbans Channel",
                "Override for unbans.",
                member,
            ),
            toggle("nickname_changes", "Nickname Changes", "Log nickname changes.", member),
            channel(
                "nickname_changes_channel_id",
                "Nickname Changes Channel",
                "Override for nickname changes.",
                member,
            ),
            toggle("role_changes", "Role Changes", "Log member role additions/removals.", member),
            channel(
                "role_changes_channel_id",
                "Role Changes Channel",
                "Override for role changes.",
                member,
            ),
        ]
    }
}
