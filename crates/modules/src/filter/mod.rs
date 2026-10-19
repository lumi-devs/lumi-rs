//! Filter module: per-guild automod rule engine (terms, regex, invites, links,
//! mention spam, caps) with delete/warn/timeout enforcement and hit logging.

pub mod enforce;
pub mod events;
pub mod heat;
pub mod store;

use lumi_kernel::module::{ConfigField, Module, ModuleMeta};

use crate::config;

#[derive(Debug)]
pub struct FilterModule;

fn string_array(key: &str, label: &str, description: &str, group: &str) -> ConfigField {
    config::string_array_field(key, label, description, group)
}

fn boolean(key: &str, label: &str, description: &str, group: &str) -> ConfigField {
    config::bool_field(key, label, description, false, group)
}

fn number(key: &str, label: &str, description: &str, default: i64, group: &str) -> ConfigField {
    config::int_field(key, label, description, default, group)
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
        let heat = "Heat Engine";
        let lockdown = "Auto-Lockdown";
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
            config::role_array_field(
                "exempt_roles",
                "Exempt Roles",
                "Roles that bypass all filter rules (members with Manage Messages are always exempt).",
                punish,
            ),
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
            config::string_field(
                "warn_message",
                "Warning Message",
                "Transient warning sent after a deletion. Placeholders: {user}, {reason}. Empty disables the warning.",
                store::DEFAULT_WARN_MESSAGE,
                punish,
            ),
            config::channel_field(
                "log_channel_id",
                "Log Channel",
                "Channel where filter hits are logged.",
                punish,
            ),
            boolean(
                "heat_enabled",
                "Enable Heat Engine",
                "Track member activity heat to detect rapid-fire spam and raids.",
                heat,
            ),
            number(
                "heat_per_message",
                "Heat Per Message",
                "Points added per message sent.",
                1,
                heat,
            ),
            number(
                "heat_per_mention",
                "Heat Per Mention",
                "Points added per user/role mention.",
                2,
                heat,
            ),
            number(
                "heat_per_filter_hit",
                "Heat Per Filter Hit",
                "Points added when a message trips any filter rule.",
                5,
                heat,
            ),
            number(
                "heat_per_attachment",
                "Heat Per Attachment",
                "Points added per file attachment.",
                2,
                heat,
            ),
            number(
                "heat_per_emoji",
                "Heat Per Emoji",
                "Points added per emoji in the message.",
                1,
                heat,
            ),
            number(
                "heat_per_link",
                "Heat Per Link",
                "Points added if message contains any link.",
                2,
                heat,
            ),
            number(
                "heat_per_zalgo",
                "Heat Per Zalgo",
                "Points added if message is heavily composed of zalgo/combining characters.",
                3,
                heat,
            ),
            number(
                "heat_decay_per_minute",
                "Heat Decay / Min",
                "Points subtracted per minute of inactivity.",
                5,
                heat,
            ),
            number(
                "heat_warn",
                "Warn Threshold",
                "Heat level that triggers an automated warning. 0 disables.",
                15,
                heat,
            ),
            number(
                "heat_timeout",
                "Timeout Threshold",
                "Heat level that triggers a timeout. 0 disables.",
                30,
                heat,
            ),
            number(
                "heat_timeout_minutes",
                "Heat Timeout Duration (m)",
                "Duration in minutes for heat-triggered timeouts.",
                10,
                heat,
            ),
            number(
                "heat_quarantine",
                "Quarantine Threshold",
                "Heat level that triggers quarantine. 0 disables.",
                50,
                heat,
            ),
            number(
                "lockdown_mention_threshold",
                "Mention Flood Threshold",
                "Mentions within the window that trigger a server-wide channel lockdown. 0 disables.",
                0,
                lockdown,
            ),
            number(
                "lockdown_window_seconds",
                "Mention Window (s)",
                "Window in seconds to aggregate mentions.",
                10,
                lockdown,
            ),
            number(
                "lockdown_duration_minutes",
                "Lockdown Duration (m)",
                "How long text channels stay locked before auto-unlocking.",
                10,
                lockdown,
            ),
        ]
    }
}
