use lumi_kernel::bot::Command;

pub mod about;
pub mod downloader;
pub mod general;
pub mod security;

pub use general::handle_component;

/// Core root commands always registered by the host engine.
pub fn core_commands() -> Vec<Command> {
    let mut commands = vec![general::ping()];
    commands.extend(about::commands());
    commands.extend(security::commands());
    commands.extend(downloader::commands());
    commands
}

/// Every command the bot registers, dynamically aggregated from the module registry.
pub fn all() -> Vec<Command> {
    let mut commands = core_commands();
    commands.extend(lumi_modules::afk::commands());
    commands.extend(lumi_modules::moderation::commands::commands());
    commands.extend(lumi_modules::tempvc::commands::commands());
    commands.extend(lumi_modules::utility::commands::commands());
    commands.extend(lumi_modules::economy::commands::commands());
    commands
}
