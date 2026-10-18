use serde_json::json;

use lumi_kernel::module::{ConfigField, ConfigFieldKind, Module, ModuleMeta};

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
            description: "Enforce server rules with warnings, mutes, kicks, bans, and cases.".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<lumi_kernel::bot::Command> {
        commands::commands()
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField {
                key: "dm_on_action".to_string(),
                label: "DM on Punish".to_string(),
                description: "Notify users directly via DM when they receive a moderation action.".to_string(),
                kind: ConfigFieldKind::Boolean,
                default: json!(true),
                subfields: Vec::new(),
                group: Some("general".to_string()),
            },
            ConfigField {
                key: "mod_log_channel".to_string(),
                label: "Moderation Log Channel".to_string(),
                description: "Channel where moderation action logs and audit records are posted.".to_string(),
                kind: ConfigFieldKind::Channel,
                default: json!(null),
                subfields: Vec::new(),
                group: Some("general".to_string()),
            },
        ]
    }
}
