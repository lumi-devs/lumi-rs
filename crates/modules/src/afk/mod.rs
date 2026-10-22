//! AFK module: set/clear status, welcome-back, mention notices, list/stats.

mod cards;
mod commands;
mod components;
mod config;
mod constants;
mod data;
mod listeners;
mod mentions;

pub use components::{PREFIX, handle_component};
pub use listeners::on_message;

use serde_json::json;

use lumi_kernel::module::{ConfigField, ConfigFieldKind, Module, ModuleMeta};

/// AFK status module.
#[derive(Debug)]
pub struct AfkModule;

impl Module for AfkModule {
    fn name(&self) -> &'static str {
        "afk"
    }

    fn metadata(&self) -> ModuleMeta {
        ModuleMeta {
            display_name: "AFK".to_string(),
            emoji: "\u{1f4a4}".to_string(),
            description:
                "Set yourself AFK; mentions notify others and a prefix is added to your nickname."
                    .to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<lumi_kernel::bot::Command> {
        commands::commands()
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField {
                key: "nick_prefix_enabled".to_string(),
                label: "Nickname Prefix".to_string(),
                description: "Prepend [AFK] to nickname while AFK.".to_string(),
                kind: ConfigFieldKind::Boolean,
                default: json!(true),
                subfields: Vec::new(),
                group: None,
            },
            ConfigField {
                key: "welcome_back_enabled".to_string(),
                label: "Welcome Back Card".to_string(),
                description: "Post a card when an AFK member returns.".to_string(),
                kind: ConfigFieldKind::Boolean,
                default: json!(true),
                subfields: Vec::new(),
                group: None,
            },
            ConfigField {
                key: "welcome_ttl_seconds".to_string(),
                label: "Welcome Card Lifetime".to_string(),
                description: "Seconds before the welcome card and its mention list are removed."
                    .to_string(),
                kind: ConfigFieldKind::Integer,
                default: json!(20),
                subfields: Vec::new(),
                group: None,
            },
        ]
    }
}
