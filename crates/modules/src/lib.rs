//! Feature modules for Lumi.

/// AFK module.
pub mod afk;
pub(crate) mod config;
/// Economy module.
pub mod economy;
/// Content filtering module.
pub mod filter;
/// Leveling and experience module.
pub mod leveling;
/// Guild audit and event logging module.
pub mod logging;
/// Moderation action module.
pub mod moderation;
pub(crate) mod patterns;
pub(crate) mod regex;
/// Security and anti-nuke module.
pub mod security;
pub(crate) mod shared;
/// Temporary voice channel module.
pub mod tempvc;
pub(crate) mod text;
/// General utility module.
pub mod utility;

pub use afk::AfkModule;
pub use economy::EconomyModule;
pub use filter::FilterModule;
pub use leveling::LevelingModule;
pub use logging::LoggingModule;
pub use moderation::ModerationModule;
pub use security::SecurityModule;
pub use tempvc::TempVcModule;
pub use utility::UtilityModule;
