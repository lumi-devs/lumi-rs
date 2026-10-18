use lumi_kernel::module::{ConfigField, Module, ModuleMeta};

pub mod commands;
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

    fn config_schema(&self) -> Vec<ConfigField> {
        Vec::new()
    }
}
