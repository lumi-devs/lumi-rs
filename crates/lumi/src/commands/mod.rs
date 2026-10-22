use lumi_kernel::bot::{Command, Context, Error};
use poise::serenity_prelude as serenity;

pub(crate) mod about;
pub(crate) mod downloader;
pub(crate) mod general;
pub(crate) mod hub;
pub(crate) mod security;

pub use hub::{handle_component as handle_hub_component, handle_modal as handle_hub_modal};

/// Resolve the interaction/command guild, or return a user-facing error.
pub fn guild_id(ctx: Context<'_>) -> Result<serenity::GuildId, Error> {
    ctx.guild_id()
        .ok_or_else(|| anyhow::anyhow!("this command can only be used in a server"))
}

/// Core root commands always registered by the host engine.
#[must_use]
pub fn core_commands() -> Vec<Command> {
    let mut commands = vec![general::ping()];
    commands.extend(hub::commands());
    commands.extend(about::commands());
    commands.extend(security::commands());
    commands.extend(downloader::commands());
    commands
}

/// Every command the bot registers, dynamically aggregated from the module registry.
#[must_use]
pub fn all_from_registry(registry: &lumi_kernel::ModuleRegistry) -> Vec<Command> {
    let mut commands = core_commands();
    for module in registry.iter() {
        commands.extend(module.commands());
    }
    commands
}
