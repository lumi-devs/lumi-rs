use lumi_kernel::bot::Command;

pub mod about;
pub mod downloader;
pub mod general;
pub mod hub;
pub mod security;

pub use general::handle_component;
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

/// Backward compatibility helper
pub fn all() -> Vec<Command> {
    let mut registry = lumi_kernel::ModuleRegistry::new();
    registry.register(Box::new(lumi_modules::afk::AfkModule));
    registry.register(Box::new(lumi_modules::logging::LoggingModule));
    registry.register(Box::new(lumi_modules::moderation::ModerationModule));
    registry.register(Box::new(lumi_modules::security::SecurityModule));
    registry.register(Box::new(lumi_modules::tempvc::TempVcModule));
    registry.register(Box::new(lumi_modules::utility::UtilityModule));
    registry.register(Box::new(lumi_modules::economy::EconomyModule));
    registry.register(Box::new(lumi_modules::leveling::LevelingModule));
    all_from_registry(&registry)
}
