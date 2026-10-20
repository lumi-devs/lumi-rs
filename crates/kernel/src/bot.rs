use crate::Services;
use std::sync::Arc;

/// Client data: shared services, reachable from any command via `ctx.data()`.
pub type Data = Arc<Services>;

pub type Error = anyhow::Error;

pub type Context<'a> = poise::Context<'a, Data, Error>;

pub type Command = poise::Command<Data, Error>;
