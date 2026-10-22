use lumi_kernel::module::{ConfigField, Module, ModuleMeta};

use crate::config;

mod commands;

pub use commands::handle_component;

/// Server utility and helper command module.
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

    fn commands(&self) -> Vec<lumi_kernel::bot::Command> {
        commands::commands()
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![config::int_field(
            "cooldown_seconds",
            "Media Cooldown (seconds)",
            "Seconds a member must wait between /avatar and /banner lookups.",
            10,
            "",
        )]
    }
}
