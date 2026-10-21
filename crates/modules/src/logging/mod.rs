//! Logging module: server event logs (messages, members, bans) as V2 cards.

pub mod events;
pub mod store;
pub mod views;

use lumi_kernel::module::{ConfigField, Module, ModuleMeta};

use crate::config::{bool_field, channel_field, string_array_field};

#[derive(Debug)]
pub struct LoggingModule;

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
            channel_field(
                "log_channel_id",
                "Default Log Channel",
                "Fallback channel for log types without their own channel. Empty disables logging.",
                Some(setup),
            ),
            channel_field(
                "message_log_channel_id",
                "Message Log Channel",
                "Channel for message deletes and edits. Falls back to the default log channel.",
                Some(message),
            ),
            string_array_field(
                "ignored_channels",
                "Ignored Channel IDs",
                "Channel IDs whose message events are not logged.",
                Some(setup),
            ),
            channel_field(
                "member_log_channel_id",
                "Member Log Channel",
                "Channel for joins, leaves, bans, unbans, nickname and role changes. Falls back to the default log channel.",
                Some(member),
            ),
            bool_field(
                "message_deletes",
                "Message Deletes",
                "Log deleted messages.",
                true,
                Some(message),
            ),
            channel_field(
                "message_deletes_channel_id",
                "Message Deletes Channel",
                "Override for deleted messages.",
                Some(message),
            ),
            bool_field(
                "message_edits",
                "Message Edits",
                "Log edited messages (before/after).",
                true,
                Some(message),
            ),
            channel_field(
                "message_edits_channel_id",
                "Message Edits Channel",
                "Override for edited messages.",
                Some(message),
            ),
            bool_field(
                "member_joins",
                "Member Joins",
                "Log members joining the server.",
                true,
                Some(member),
            ),
            channel_field(
                "member_joins_channel_id",
                "Member Joins Channel",
                "Override for member joins.",
                Some(member),
            ),
            bool_field(
                "member_leaves",
                "Member Leaves",
                "Log members leaving the server.",
                true,
                Some(member),
            ),
            channel_field(
                "member_leaves_channel_id",
                "Member Leaves Channel",
                "Override for member leaves.",
                Some(member),
            ),
            bool_field(
                "member_bans",
                "Member Bans",
                "Log bans.",
                true,
                Some(member),
            ),
            channel_field(
                "member_bans_channel_id",
                "Member Bans Channel",
                "Override for bans.",
                Some(member),
            ),
            bool_field(
                "member_unbans",
                "Member Unbans",
                "Log unbans.",
                true,
                Some(member),
            ),
            channel_field(
                "member_unbans_channel_id",
                "Member Unbans Channel",
                "Override for unbans.",
                Some(member),
            ),
            bool_field(
                "nickname_changes",
                "Nickname Changes",
                "Log nickname changes.",
                true,
                Some(member),
            ),
            channel_field(
                "nickname_changes_channel_id",
                "Nickname Changes Channel",
                "Override for nickname changes.",
                Some(member),
            ),
            bool_field(
                "role_changes",
                "Role Changes",
                "Log member role additions/removals.",
                true,
                Some(member),
            ),
            channel_field(
                "role_changes_channel_id",
                "Role Changes Channel",
                "Override for role changes.",
                Some(member),
            ),
        ]
    }
}
