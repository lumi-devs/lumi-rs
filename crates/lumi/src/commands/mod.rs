use lumi_kernel::bot::Command;

pub mod general;

pub use general::handle_component;

/// Every command the bot registers, aggregated from its modules.
pub fn all() -> Vec<Command> {
    let mut commands = vec![general::ping()];
    commands.extend(lumi_modules::afk::commands());
    commands
}
