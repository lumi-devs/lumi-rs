//! Core runtime kernel for Lumi.

/// Bot framework types and context.
pub mod bot;
/// In-process and Redis distributed caching.
pub mod cache;
/// Kernel configuration and role definitions.
pub mod config;
/// Shard coordination and lease management.
pub mod coordinator;
/// Addon repository downloader and updater.
pub mod downloader;
/// Kernel errors and failure classification.
pub mod error;
/// Module definitions and registry.
pub mod module;
/// Per-guild module configuration and state.
pub mod module_state;
/// Granular permission checking and resolution.
pub mod permits;
/// Command prefix management and caching.
pub mod prefix;
/// Transient operation retries and timeouts.
pub mod retry;
/// Job scheduling and background worker.
pub mod scheduler;
/// Antinuke protection and security manager.
pub mod security;
/// Shared runtime dependency container.
pub mod services;
pub use dashmap;
pub use services::Services;

pub use bot::{Command, Context, Data, Error as BotError};
pub use cache::Cache;
pub use config::{Config, Role};
pub use downloader::Downloader;
pub use error::{FailureClass, LumiError, Result};
pub use module::{ConfigField, ConfigFieldKind, Module, ModuleMeta, ModuleRegistry};
pub use permits::{PermitKind, PermitPolarity, PermitResolver, PermitTargetType};
pub use retry::{retry_transient, with_timeout};
pub use scheduler::{Job, Scheduler};
pub use security::{
    ActionThresholdViolation, AntiNukeConfig, AntiNukeResponse, DestructiveActionKind, PanicStatus,
    SecurityManager,
};
