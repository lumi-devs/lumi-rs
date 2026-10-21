//! AFK module: set/clear status, welcome-back, mention notices, list/stats.

mod cards;
mod commands;
mod components;
mod config;
mod constants;
mod data;
mod listeners;
mod mentions;

pub use components::{PREFIX, handle_component};
pub use listeners::on_message;

use lumi_kernel::module::{ConfigField, Module, ModuleMeta};

use crate::config::{bool_field, int_field};

#[derive(Debug)]
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

    fn commands(&self) -> Vec<lumi_kernel::bot::Command> {
        commands::commands()
    }

    fn config_schema(&self) -> Vec<ConfigField> {
        vec![
            bool_field(
                "nick_prefix_enabled",
                "Nickname Prefix",
                "Prepend [AFK] to nickname while AFK.",
                true,
                None,
            ),
            bool_field(
                "welcome_back_enabled",
                "Welcome Back Card",
                "Post a card when an AFK member returns.",
                true,
                None,
            ),
            int_field(
                "welcome_ttl_seconds",
                "Welcome Card Lifetime",
                "Seconds before the welcome card and its mention list are removed.",
                i64::from(constants::DEFAULT_WELCOME_TTL_SECS),
                None,
            ),
        ]
    }
}
