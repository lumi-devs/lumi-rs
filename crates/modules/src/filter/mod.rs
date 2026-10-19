//! Filter module: per-guild automod rule engine (terms, regex, invites, links,
//! mention spam, caps) with delete/warn/timeout enforcement and hit logging.

pub mod enforce;
pub mod events;
pub mod store;

use serde_json::json;

use lumi_kernel::module::{ConfigField, ConfigFieldKind, Module, ModuleMeta};

#[derive(Debug)]
pub struct FilterModule;

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

fn boolean(key: &str, label: &str, description: &str, group: &str) -> ConfigField {
    ConfigField {
        key: key.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        kind: ConfigFieldKind::Boolean,
        default: json!(false),
        subfields: Vec::new(),
        group: Some(group.to_string()),
    }
}

fn number(key: &str, label: &str, description: &str, default: i64, group: &str) -> ConfigField {
    ConfigField {
        key: key.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        kind: ConfigFieldKind::Integer,
        default: json!(default),
        subfields: Vec::new(),
        group: Some(group.to_string()),
    }
}

impl Module for FilterModule {
    fn name(&self) -> &'static str {
        "filter"
    }

    fn metadata(&self) -> ModuleMeta {
        ModuleMeta {
            display_name: "Filter".to_string(),
            emoji: "\u{1f6ab}".to_string(),
            description: "Automod rule engine: filtered terms, regex rules, invite/link blocking with allowlists, mention-spam and caps limits, with configurable punishment and logging.".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        let terms = "Terms & Patterns";
        let links = "Invites & Links";
        let spam = "Spam Limits";
        let punish = "Punishment";
        vec![
            string_array(
                "terms",
                "Filtered Terms",
                "Words/phrases to block (case-insensitive substring), one per line.",
                terms,
            ),
            string_array(
                "regex_rules",
                "Regex Rules",
                "Regular expressions to block (case-insensitive, max 256 chars each), one per line. Invalid patterns are skipped.",
                terms,
            ),
            boolean(
                "block_invites",
                "Block Invites",
                "Delete messages containing Discord server invites.",
                links,
            ),
            string_array(
                "invite_allowlist",
                "Allowed Invite Codes",
                "Invite codes that are always allowed (e.g. your own server's), one per line.",
                links,
            ),
            boolean(
                "block_links",
                "Block Links",
                "Delete messages containing links, except allowlisted domains.",
                links,
            ),
            string_array(
                "link_allowlist",
                "Allowed Link Domains",
                "Domains exempt from link blocking (subdomains included), one per line, e.g. youtube.com.",
                links,
            ),
            number(
                "max_mentions",
                "Max Mentions",
                "Maximum user+role mentions per message. 0 disables the rule.",
                0,
                spam,
            ),
            number(
                "max_caps_percent",
                "Max Caps %",
                "Delete messages whose letters are more than this % uppercase. 0 disables the rule.",
                0,
                spam,
            ),
            number(
                "caps_min_length",
                "Caps Min Length",
                "Messages shorter than this never trip the caps rule.",
                12,
                spam,
            ),
            ConfigField {
                key: "exempt_roles".to_string(),
                label: "Exempt Roles".to_string(),
                description: "Roles that bypass all filter rules (members with Manage Messages are always exempt).".to_string(),
                kind: ConfigFieldKind::RoleArray,
                default: json!([]),
                subfields: Vec::new(),
                group: Some(punish.to_string()),
            },
            number(
                "timeout_minutes",
                "Timeout (minutes)",
                "Timeout the author for this many minutes on a filter hit. 0 disables punishment.",
                0,
                punish,
            ),
            number(
                "spam_timeout_minutes",
                "Spam Timeout (minutes)",
                "Timeout for mention-spam/caps hits. 0 falls back to Timeout (minutes).",
                0,
                punish,
            ),
            number(
                "invite_timeout_minutes",
                "Invite Timeout (minutes)",
                "Timeout for invite-block hits. 0 falls back to Timeout (minutes).",
                0,
                punish,
            ),
            number(
                "link_timeout_minutes",
                "Link Timeout (minutes)",
                "Timeout for link-block hits. 0 falls back to Timeout (minutes).",
                0,
                punish,
            ),
            ConfigField {
                key: "warn_message".to_string(),
                label: "Warning Message".to_string(),
                description: "Transient warning sent after a deletion. Placeholders: {user}, {reason}. Empty disables the warning.".to_string(),
                kind: ConfigFieldKind::String,
                default: json!(store::DEFAULT_WARN_MESSAGE),
                subfields: Vec::new(),
                group: Some(punish.to_string()),
            },
            ConfigField {
                key: "log_channel_id".to_string(),
                label: "Log Channel".to_string(),
                description: "Channel where filter hits are logged.".to_string(),
                kind: ConfigFieldKind::Channel,
                default: json!(null),
                subfields: Vec::new(),
                group: Some(punish.to_string()),
            },
        ]
    }
}
