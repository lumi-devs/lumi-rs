//! Dynamic Temporary Voice Channels module.

mod commands;
mod events;
pub(crate) mod store;
mod views;

pub use commands::{handle_component, handle_modal};
pub use events::on_voice_state_update;

/// True when a REST error means the target channel is already gone.
pub(crate) fn channel_missing(error: &poise::serenity_prelude::Error) -> bool {
    matches!(
        error,
        poise::serenity_prelude::Error::Http(http)
            if http.status_code().is_some_and(|code| code.as_u16() == 404)
    )
}

use lumi_kernel::module::{ConfigField, Module, ModuleMeta};

/// Temporary voice channels module.
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
