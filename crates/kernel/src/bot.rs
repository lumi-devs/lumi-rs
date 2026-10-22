use crate::Services;
use std::sync::Arc;

/// Client data: shared services, reachable from any command via `ctx.data()`.
pub type Data = Arc<Services>;

/// Command handler error type.
pub type Error = anyhow::Error;

/// Command execution context.
pub type Context<'a> = poise::Context<'a, Data, Error>;

/// Bot command definition.
pub type Command = poise::Command<Data, Error>;
