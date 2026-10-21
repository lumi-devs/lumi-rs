//! Dynamic Temporary Voice Channels module.

pub mod commands;
pub mod events;
pub mod store;
pub mod views;

pub use commands::{commands, handle_component, handle_modal};
pub use events::on_voice_state_update;

use lumi_kernel::module::{ConfigField, Module, ModuleMeta};

/// Discord's hard limit for channel names.
pub(super) const MAX_CHANNEL_NAME: usize = 100;

#[derive(Debug)]
pub struct TempVcModule;

impl Module for TempVcModule {
    fn name(&self) -> &'static str {
        "tempvc"
    }

    fn metadata(&self) -> ModuleMeta {
        ModuleMeta {
            display_name: "Temporary Voice Channels".to_string(),
            emoji: "🔊".to_string(),
            description: "Automatically create and manage dynamic temporary voice channels."
                .to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    fn commands(&self) -> Vec<lumi_kernel::bot::Command> {
        commands::commands()
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![]
    }
}
