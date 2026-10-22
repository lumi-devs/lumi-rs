use lumi_kernel::module::{ConfigField, Module, ModuleMeta};

mod commands;
mod events;
mod images;
pub(crate) mod store;
mod views;

pub use events::{on_message, on_voice_state_update};

/// Member leveling and XP progression module.
#[derive(Debug)]
pub struct LevelingModule;

impl Module for LevelingModule {
    fn name(&self) -> &'static str {
        "leveling"
    }

    fn metadata(&self) -> ModuleMeta {
        ModuleMeta {
            display_name: "Leveling".to_string(),
            emoji: "⭐".to_string(),
            description: "Server experience, level progression, rank cards, and leaderboards."
                .to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<lumi_kernel::bot::Command> {
        commands::commands()
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        Vec::new()
    }
}
