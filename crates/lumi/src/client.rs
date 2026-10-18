use std::sync::Arc;

use anyhow::Result;
use poise::serenity_prelude as serenity;

pub use lumi_kernel::bot::{Context, Data, Error};

pub async fn build(cfg: lumi_kernel::Config) -> Result<serenity::Client> {
    let db = lumi_db::connect(&cfg.database_url).await?;
    lumi_db::migrate(&db).await?;

    let cache = Arc::new(lumi_kernel::Cache::connect(&cfg.redis_url).await?);
    let permits = Arc::new(lumi_kernel::PermitResolver::load(&db).await?);
    let mut registry = lumi_kernel::ModuleRegistry::new();
    registry.register(Box::new(lumi_modules::afk::AfkModule));
    registry.register(Box::new(lumi_modules::logging::LoggingModule));
    registry.register(Box::new(lumi_modules::moderation::ModerationModule));
    registry.register(Box::new(lumi_modules::utility::UtilityModule));

    let services: Data = Arc::new(lumi_kernel::Services {
        cfg,
        db,
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
