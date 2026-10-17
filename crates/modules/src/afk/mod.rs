//! AFK module: set/clear status, welcome-back, mention notices, list/stats.

mod cards;
mod commands;
mod data;
mod listeners;

pub use cards::{mention_card, welcome_back_card};
pub use commands::commands;
pub use data::{clear, count, get, list, set};
pub use listeners::on_message;

use serde_json::json;

use lumi_kernel::module::{ConfigField, ConfigFieldKind, Module, ModuleMeta};

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

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![ConfigField {
            key: "nick_prefix_enabled".to_string(),
            label: "Nickname Prefix".to_string(),
            description: "Prepend [AFK] to nickname while AFK.".to_string(),
            kind: ConfigFieldKind::Boolean,
            default: json!(true),
            subfields: Vec::new(),
            group: None,
        }]
    }
}
