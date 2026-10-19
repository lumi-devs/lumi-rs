use std::sync::Arc;

use anyhow::Result;
use poise::serenity_prelude as serenity;

pub use lumi_kernel::bot::{Context, Data, Error};

/// Permit gate for guild commands. Commands absent from `permits::required_node`
/// pass through untouched; permitted ones get an ephemeral denial card.
fn command_check<'a>(ctx: Context<'a>) -> poise::BoxFuture<'a, Result<bool, Error>> {
    Box::pin(async move {
        let Some(guild_id) = ctx.guild_id() else {
            return Ok(true);
        };
        let Some(root) = ctx.command_tree().first() else {
            return Ok(true);
        };
        let Some(node) = lumi_kernel::permits::required_node(&root.name) else {
            return Ok(true);
        };

        let author_id = ctx.author().id;
        let mut role_ids: Vec<u64> = Vec::new();
        let mut is_guild_owner = false;
        let mut is_manager = false;
        if let Some(guild) = ctx.guild() {
            is_guild_owner = guild.owner_id == author_id;
            if let Some(member) = guild.members.get(&author_id) {
                let perms = guild.member_permissions(member);
                is_manager = perms.contains(serenity::all::Permissions::ADMINISTRATOR)
                    || perms.contains(serenity::all::Permissions::MANAGE_GUILD);
                role_ids = member.roles.iter().map(|role| role.get()).collect();
                role_ids.sort_by_key(|id| {
                    guild
                        .roles
                        .get(&serenity::all::RoleId::new(*id))
                        .map_or(0, |role| role.position)
                });
                role_ids.reverse();
            }
        }

        let is_quarantined =
            lumi_modules::security::store::panic_locks_commands(&ctx.data(), guild_id.get()).await;

        let check = lumi_kernel::permits::CheckContext {
            guild_id: guild_id.get().to_string(),
            user_id: author_id.get(),
            channel_id: Some(ctx.channel_id().get()),
            role_ids: &role_ids,
            is_owner: ctx.data().cfg.is_owner(author_id.get()),
            is_guild_owner,
            is_manager,
            is_quarantined,
            node,
        };
        if ctx.data().permits.resolve(&check).allowed {
            return Ok(true);
        }

        let card = lumi_ui::Card::error(
            "Permission Denied",
            format!("You lack the required permit (`{node}`) to use this."),
        )
        .ephemeral();
        ctx.send(card.into_reply().reply()).await?;
        Ok(false)
    })
}

pub async fn build(cfg: lumi_kernel::Config) -> Result<serenity::Client> {
    let db = lumi_db::connect(&cfg.database_url).await?;
    lumi_db::migrate(&db).await?;

    let cache = Arc::new(lumi_kernel::Cache::connect(&cfg.redis_url).await?);
    let permits = Arc::new(lumi_kernel::PermitResolver::load(&db).await?);
    let mut registry = lumi_kernel::ModuleRegistry::new();
    registry.register(Box::new(lumi_modules::afk::AfkModule));
    registry.register(Box::new(lumi_modules::logging::LoggingModule));
    registry.register(Box::new(lumi_modules::filter::FilterModule));
    registry.register(Box::new(lumi_modules::moderation::ModerationModule));
    registry.register(Box::new(lumi_modules::security::SecurityModule));
    registry.register(Box::new(lumi_modules::tempvc::TempVcModule));
    registry.register(Box::new(lumi_modules::utility::UtilityModule));
    registry.register(Box::new(lumi_modules::economy::EconomyModule));
    registry.register(Box::new(lumi_modules::leveling::LevelingModule));

    let security = Arc::new(lumi_kernel::SecurityManager::new(db.clone()));
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
        downloader,
    });

    tokio::spawn(async move { cache.spawn_invalidation().await });

    let framework = poise::Framework::new(poise::FrameworkOptions {
        commands,
        command_check: Some(command_check),
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
