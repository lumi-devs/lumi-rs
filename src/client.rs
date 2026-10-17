use std::sync::Arc;

use anyhow::Result;
use poise::serenity_prelude as serenity;

pub use lumi::bot::{Context, Data, Error};

pub async fn build(cfg: lumi::Config) -> Result<serenity::Client> {
    let db = lumi::db::connect(&cfg.database_url).await?;
    lumi::db::migrate(&db).await?;
    let addons = Arc::new(lumi::addon::AddonHost::load(std::path::Path::new(
        &cfg.custom_dir,
    ))?);

    let cache = Arc::new(lumi::cache::Cache::connect(&cfg.redis_url).await?);
    let permits = Arc::new(lumi::permits::PermitResolver::load(&db).await?);
    let registry = lumi::module::ModuleRegistry::new();

    let services = Arc::new(lumi::Services {
        cfg,
        db,
        addons,
        cache: Arc::clone(&cache),
        permits,
        registry,
    });

    tokio::spawn(async move { cache.spawn_invalidation().await });

    let framework = poise::Framework::new(poise::FrameworkOptions {
        commands: crate::commands::all(),
        ..Default::default()
    });

    let intents = serenity::GatewayIntents::non_privileged()
        | serenity::GatewayIntents::MESSAGE_CONTENT
        | serenity::GatewayIntents::GUILD_MEMBERS;

    let token: serenity::Token = services.cfg.token.parse()?;

    let client = serenity::ClientBuilder::new(token, intents)
        .data(services)
        .framework(Box::new(framework))
        .event_handler(Arc::new(crate::events::Events))
        .await?;

    Ok(client)
}
