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
    registry.register(Box::new(lumi_modules::tempvc::TempVcModule));
    registry.register(Box::new(lumi_modules::utility::UtilityModule));
    registry.register(Box::new(lumi_modules::economy::EconomyModule));
    registry.register(Box::new(lumi_modules::leveling::LevelingModule));

    let security = Arc::new(lumi_kernel::SecurityManager::new(db.clone()));
    let filter = Arc::new(lumi_kernel::ContentFilter::default());
    let downloader = Arc::new(lumi_kernel::Downloader::new(
        db.clone(),
        std::path::PathBuf::from(&cfg.custom_dir),
    ));

    let commands = crate::commands::all_from_registry(&registry);

    let services: Data = Arc::new(lumi_kernel::Services {
        cfg,
        db,
        cache: Arc::clone(&cache),
        permits,
        registry,
        security,
        filter,
        downloader,
    });

    tokio::spawn(async move { cache.spawn_invalidation().await });

    let framework = poise::Framework::new(poise::FrameworkOptions {
        commands,
        prefix_options: poise::PrefixFrameworkOptions {
            prefix: Some(",".into()),
            mention_as_prefix: true,
            dynamic_prefix: Some(|ctx| {
                Box::pin(async move {
                    let data = ctx.framework.user_data();
                    let db = &data.db;
                    let guild_id = ctx.guild_id.map(|g| g.get());
                    let p = lumi_db::get_prefix(db, guild_id).await;
                    Ok(Some(p.into()))
                })
            }),
            case_insensitive_commands: true,
            ..Default::default()
        },
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
