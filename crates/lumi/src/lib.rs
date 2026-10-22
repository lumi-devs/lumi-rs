//! Lumi Discord bot runtime and commands.

use clap as _;
use dotenvy as _;
use lumi_api as _;
use mimalloc as _;
use rustls as _;
use tracing_subscriber as _;

/// Discord client and framework initialization.
pub mod client;
/// Codebase analytics and statistics.
pub mod codebase;
/// Application commands.
pub mod commands;
/// Error reporting and telemetry.
pub mod errors;
/// Gateway event handlers.
pub mod events;
/// Discord component and modal interaction routing.
pub mod interactions;
/// Ping diagnostic dashboard.
pub mod ping;
/// Runtime startup, services, and worker loops.
pub mod runtime;
