use lumi_kernel::module::{ConfigField, Module, ModuleMeta};

use crate::config;

pub mod actions;
pub mod channels;
pub mod commands;
pub mod store;
pub mod views;

#[derive(Debug)]
pub struct ModerationModule;

impl Module for ModerationModule {
    fn name(&self) -> &'static str {
        "moderation"
    }

    fn metadata(&self) -> ModuleMeta {
        ModuleMeta {
            display_name: "Moderation".to_string(),
            emoji: "🛡️".to_string(),
            description: "Enforce server rules with warnings, mutes, kicks, bans, and cases."
                .to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<lumi_kernel::bot::Command> {
        commands::commands()
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            config::bool_field(
                "dm_on_action",
                "DM on Punish",
                "Notify users directly via DM when they receive a moderation action.",
                true,
                "general",
            ),
            config::channel_field(
                "mod_log_channel",
                "Moderation Log Channel",
                "Channel where moderation action logs and audit records are posted.",
                "general",
            ),
            config::role_field(
                "quarantine_role_id",
                "Quarantine Role",
                "Role applied by /quarantine and anti-nuke enforcement.",
                "general",
            ),
        ]
    }
}
