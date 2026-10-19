use lumi_kernel::module::{ConfigField, Module, ModuleMeta};

pub mod commands;
pub mod events;
pub mod images;
pub mod store;
pub mod views;

pub use commands::commands;
pub use events::{on_message, on_voice_state_update};

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
            description: "Server experience, level progression, rank cards, and leaderboards.".to_string(),
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
