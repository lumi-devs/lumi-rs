//! Security module: audit-log anti-nuke enforcement and panic command gating.
//!
//! The `/panic` command itself stays in the host binary (`commands::security`);
//! this module owns the config schema, the anti-nuke store and the listeners.

pub mod events;
pub mod store;

use serde_json::json;

use lumi_kernel::module::{ConfigField, ConfigFieldKind, Module, ModuleMeta};

#[derive(Debug)]
pub struct SecurityModule;

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

fn response(key: &str, label: &str, description: &str, group: &str) -> ConfigField {
    ConfigField {
        key: key.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        kind: ConfigFieldKind::Enum(vec![
            "log".to_string(),
            "quarantine".to_string(),
            "ban".to_string(),
        ]),
        default: json!("quarantine"),
        subfields: Vec::new(),
        group: Some(group.to_string()),
    }
}

impl Module for SecurityModule {
    fn name(&self) -> &'static str {
        "security"
    }

    fn metadata(&self) -> ModuleMeta {
        ModuleMeta {
            display_name: "Security".to_string(),
            emoji: "\u{1f6e1}\u{fe0f}".to_string(),
            description: "Server protection: anti-nuke detection with automatic quarantine."
                .to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        let anti = "Anti-Nuke";
        let limits = "Nuke Limits";
        let panic = "Panic Mode";
        vec![
            ConfigField {
                key: "antinuke_enabled".to_string(),
                label: "Anti-Nuke".to_string(),
                description: "Watch the audit log for mass destructive actions.".to_string(),
                kind: ConfigFieldKind::Boolean,
                default: json!(false),
                subfields: Vec::new(),
                group: Some(anti.to_string()),
            },
            number(
                "window_seconds",
                "Detection Window",
                "Sliding window in seconds for counting actions.",
                60,
                anti,
            ),
            ConfigField {
                key: "trusted_role_ids".to_string(),
                label: "Trusted Roles".to_string(),
                description: "Role IDs exempt from anti-nuke.".to_string(),
                kind: ConfigFieldKind::StringArray,
                default: json!([]),
                subfields: Vec::new(),
                group: Some(anti.to_string()),
            },
            ConfigField {
                key: "log_channel_id".to_string(),
                label: "Security Log Channel".to_string(),
                description: "Channel for anti-nuke alerts.".to_string(),
                kind: ConfigFieldKind::Channel,
                default: json!(null),
                subfields: Vec::new(),
                group: Some(anti.to_string()),
            },
            number("max_bans", "Max Bans", "Bans allowed per executor within the window.", 5, limits),
            response("response_bans", "Response - Bans", "Action taken when the ban threshold trips.", limits),
            number("max_kicks", "Max Kicks", "Kicks allowed per executor within the window.", 5, limits),
            response("response_kicks", "Response - Kicks", "Action taken when the kick threshold trips.", limits),
            number(
                "max_channel_deletes",
                "Max Channel Deletes",
                "Channel deletions allowed per executor within the window.",
                3,
                limits,
            ),
            response(
                "response_channel_deletes",
                "Response - Channel Deletes",
                "Action taken when the channel-delete threshold trips.",
                limits,
            ),
            number(
                "max_role_deletes",
                "Max Role Deletes",
                "Role deletions allowed per executor within the window.",
                3,
                limits,
            ),
            response(
                "response_role_deletes",
                "Response - Role Deletes",
                "Action taken when the role-delete threshold trips.",
                limits,
            ),
            number(
                "max_webhook_creates",
                "Max Webhook Creates",
                "Webhook creations allowed per executor within the window.",
                3,
                limits,
            ),
            response(
                "response_webhook_creates",
                "Response - Webhook Creates",
                "Action taken when the webhook-create threshold trips.",
                limits,
            ),
            number(
                "max_vanity_changes",
                "Max Vanity URL Changes",
                "Vanity invite code changes allowed per executor within the window.",
                1,
                limits,
            ),
            number(
                "max_permission_grants",
                "Max Dangerous Permission Grants",
                "Times @everyone can be granted a dangerous permission before the response fires.",
                1,
                limits,
            ),
            number(
                "max_quarantine_bypass",
                "Max Quarantine Bypass Attempts",
                "Times a quarantined member's roles can be changed before the response fires.",
                1,
                limits,
            ),
            ConfigField {
                key: "panic_lock_mod_commands".to_string(),
                label: "Lock Mod Commands During Panic".to_string(),
                description: "While panic mode is active, only managers may run permission-gated commands.".to_string(),
                kind: ConfigFieldKind::Boolean,
                default: json!(true),
                subfields: Vec::new(),
                group: Some(panic.to_string()),
            },
        ]
    }
}
