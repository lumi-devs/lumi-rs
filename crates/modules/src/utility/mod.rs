use lumi_kernel::module::{ConfigField, ConfigFieldKind, Module, ModuleMeta};

pub mod commands;

pub use commands::handle_component;

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
        vec![ConfigField {
            key: "cooldown_seconds".to_string(),
            label: "Media Cooldown (seconds)".to_string(),
            description: "Seconds a member must wait between /avatar and /banner lookups."
                .to_string(),
            kind: ConfigFieldKind::Integer,
            default: serde_json::json!(10),
            subfields: Vec::new(),
            group: None,
        }]
    }
}
