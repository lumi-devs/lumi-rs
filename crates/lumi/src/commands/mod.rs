use lumi_kernel::bot::Command;

pub mod about;
pub mod downloader;
pub mod general;
pub mod hub;
pub mod security;

pub use hub::{handle_component as handle_hub_component, handle_modal as handle_hub_modal};

/// Core root commands always registered by the host engine.
pub fn core_commands() -> Vec<Command> {
    let mut commands = vec![general::ping()];
    commands.extend(hub::commands());
    commands.extend(about::commands());
    commands.extend(security::commands());
    commands.extend(downloader::commands());
    commands
}

/// Every command the bot registers, dynamically aggregated from the module registry.
pub fn all_from_registry(registry: &lumi_kernel::ModuleRegistry) -> Vec<Command> {
    let mut commands = core_commands();
    for module in registry.iter() {
        commands.extend(module.commands());
    }
    commands
}
