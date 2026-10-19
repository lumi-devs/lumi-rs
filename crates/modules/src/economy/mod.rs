use lumi_kernel::module::{ConfigField, ConfigFieldKind, Module, ModuleMeta};

pub mod commands;
pub mod config;
pub mod store;

#[derive(Debug)]
pub struct EconomyModule;

impl Module for EconomyModule {
    fn name(&self) -> &'static str {
        "economy"
    }

    fn metadata(&self) -> ModuleMeta {
        ModuleMeta {
            display_name: "Economy".to_string(),
            emoji: "🪙".to_string(),
            description: "Server currency system with wallets, daily allowances, and transfers.".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<lumi_kernel::bot::Command> {
        commands::commands()
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            ConfigField {
                key: "currency_name".to_string(),
                label: "Currency Name".to_string(),
                description: "What the currency is called (shown after amounts).".to_string(),
                kind: ConfigFieldKind::String,
                default: serde_json::json!("credits"),
                subfields: Vec::new(),
                group: None,
            },
            ConfigField {
                key: "currency_emoji".to_string(),
                label: "Currency Emoji".to_string(),
                description: "Emoji shown before amounts.".to_string(),
                kind: ConfigFieldKind::String,
                default: serde_json::json!("🪙"),
                subfields: Vec::new(),
                group: None,
            },
            ConfigField {
                key: "bank_name".to_string(),
                label: "Bank Label".to_string(),
                description: "What the bank balance is called.".to_string(),
                kind: ConfigFieldKind::String,
                default: serde_json::json!("Bank"),
                subfields: Vec::new(),
                group: None,
            },
        ]
    }
}
