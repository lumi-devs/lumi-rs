//! Security module: audit-log anti-nuke enforcement and panic command gating.
//!
//! The `/panic` command itself stays in the host binary (`commands::security`);
//! this module owns the config schema, the anti-nuke store and the listeners.

pub mod events;
pub mod store;

use lumi_kernel::module::{ConfigField, Module, ModuleMeta};

use crate::shared::{bool_field, channel_field, enum_field, int_field, string_array_field};

#[derive(Debug)]
pub struct SecurityModule;

fn number(key: &str, label: &str, description: &str, default: i64, group: &str) -> ConfigField {
    int_field(key, label, description, default, Some(group))
}

fn response(key: &str, label: &str, description: &str, group: &str) -> ConfigField {
    enum_field(
        key,
        label,
        description,
        &["log", "quarantine", "ban"],
        "quarantine",
        Some(group),
    )
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
            bool_field(
                "antinuke_enabled",
                "Anti-Nuke",
                "Watch the audit log for mass destructive actions.",
                false,
                Some(anti),
            ),
            number(
                "window_seconds",
                "Detection Window",
                "Sliding window in seconds for counting actions.",
                60,
                anti,
            ),
            string_array_field(
                "trusted_role_ids",
                "Trusted Roles",
                "Role IDs exempt from anti-nuke.",
                Some(anti),
            ),
            channel_field(
                "log_channel_id",
                "Security Log Channel",
                "Channel for anti-nuke alerts.",
                Some(anti),
            ),
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
            bool_field(
                "panic_lock_mod_commands",
                "Lock Mod Commands During Panic",
                "While panic mode is active, only managers may run permission-gated commands.",
                true,
                Some(panic),
            ),
        ]
    }
}
