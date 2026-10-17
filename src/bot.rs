//! Shared bot types so modules can own their commands (RedBot/lumi-style
//! containment). The bin re-exports these from `crate::client`.

use std::sync::Arc;

use crate::Services;

/// Client data: shared services, reachable from any command via `ctx.data()`.
pub type Data = Arc<Services>;

pub type Error = anyhow::Error;

pub type Context<'a> = poise::Context<'a, Data, Error>;

pub type Command = poise::Command<Data, Error>;
