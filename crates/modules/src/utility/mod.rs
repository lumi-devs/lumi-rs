use lumi_kernel::module::{ConfigField, Module, ModuleMeta};

pub mod commands;

#[derive(Debug)]
pub struct UtilityModule;

impl Module for UtilityModule {
    fn name(&self) -> &'static str {
        "utility"
    }

    fn metadata(&self) -> ModuleMeta {
        ModuleMeta {
            display_name: "Utility".to_string(),
            emoji: "🛠️".to_string(),
            description: "Essential server management tools and info lookup commands.".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        Vec::new()
    }
}
