//! Shared process wiring: module registry, service construction and supervision.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use lumi_kernel::bot::Data;
use lumi_kernel::{Config, ModuleRegistry, Services};

/// Connect to the database with retries.
async fn connect_db(url: &str) -> Result<lumi_db::DatabaseConnection> {
    let url = url.to_owned();
    lumi_kernel::retry_transient(3, Duration::from_millis(250), || {
        let url = url.clone();
        async move {
            lumi_db::connect(&url)
                .await
                .map_err(lumi_kernel::LumiError::from)
        }
    })
    .await
    .map_err(anyhow::Error::from)
}

/// Every feature module the host knows about, in display order.
#[must_use]
pub fn registry() -> ModuleRegistry {
    let mut registry = ModuleRegistry::new();
    registry.register(Box::new(lumi_modules::afk::AfkModule));
    registry.register(Box::new(lumi_modules::logging::LoggingModule));
    registry.register(Box::new(lumi_modules::filter::FilterModule));
    registry.register(Box::new(lumi_modules::moderation::ModerationModule));
    registry.register(Box::new(lumi_modules::security::SecurityModule));
    registry.register(Box::new(lumi_modules::tempvc::TempVcModule));
    registry.register(Box::new(lumi_modules::utility::UtilityModule));
    registry.register(Box::new(lumi_modules::economy::EconomyModule));
    registry.register(Box::new(lumi_modules::leveling::LevelingModule));
    registry
}

/// How often the always-on backend latency probes run.
const BACKEND_PROBE_INTERVAL: Duration = Duration::from_secs(15);

fn spawn_backend_probe(services: &Data) {
    let services = Arc::clone(services);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(BACKEND_PROBE_INTERVAL);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;

            let started = std::time::Instant::now();
            if services.db.ping().await.is_ok() {
                metrics::gauge!("lumi.db.latency_ms").set(started.elapsed().as_secs_f64() * 1000.0);
            } else {
                metrics::counter!("lumi.db.probe_failed").increment(1);
            }

            match services.cache.probe_read_ms().await {
                Ok(ms) => metrics::gauge!("lumi.redis.latency_ms").set(ms),
                Err(_) => metrics::counter!("lumi.redis.probe_failed").increment(1),
            }
        }
    });
}

/// Bounded drain deadline during SIGTERM or graceful shutdown.
pub const SHUTDOWN_DRAIN_TIMEOUT: Duration = Duration::from_secs(15);
/// Recurring addon auto-update job owned by the worker tier.
pub const ADDONS_AUTOUPDATE_JOB: &str = "addons.autoupdate";
/// How often the worker checks for scheduled jobs.
pub const WORKER_TICK: Duration = Duration::from_secs(60);
const AUTO_UPDATE_LOCK: &str = "lumi:job:addons.autoupdate";

/// Parse a process role from its command-line name.
pub fn role_from_arg(arg: &str) -> Result<lumi_kernel::Role> {
    match arg.to_ascii_lowercase().as_str() {
        "all" => Ok(lumi_kernel::Role::All),
        "gateway" | "shard" => Ok(lumi_kernel::Role::Gateway),
        "worker" => Ok(lumi_kernel::Role::Worker),
        "api" => Ok(lumi_kernel::Role::Api),
        "scheduler" => Err(anyhow::anyhow!(
            "the 'scheduler' role has been merged into 'worker'; run with --role worker instead"
        )),
        other => Err(anyhow::anyhow!(
            "unknown role '{other}'; valid options are all, gateway, worker, api"
        )),
    }
}

/// Run one background job by name.
pub async fn run_job(services: &Services, job: lumi_kernel::Job) -> lumi_kernel::Result<()> {
    match job.name.as_str() {
        ADDONS_AUTOUPDATE_JOB => auto_update_addons(services).await,
        other => Err(lumi_kernel::LumiError::Invalid(format!(
            "unknown background job: {other}"
        ))),
    }
}

async fn auto_update_addons(services: &Services) -> lumi_kernel::Result<()> {
    let cfg = services.downloader.get_auto_update_config().await?;
    if !cfg.enabled {
        return Ok(());
    }

    let interval_ms = cfg.interval_minutes.max(1).saturating_mul(60_000);
    if !services
        .cache
        .claim_cooldown(AUTO_UPDATE_LOCK, interval_ms)
        .await?
    {
        return Ok(());
    }

    for repo in services.downloader.check_for_updates_detailed().await? {
        match services.downloader.update_repo(&repo).await {
            Ok(result) if result.changed => {
                tracing::info!(repo = %repo, "addon repository auto-updated");
            }
            Ok(_) => {}
            Err(err) => {
                tracing::warn!(repo = %repo, error = %err, "addon auto-update failed");
            }
        }
    }
    Ok(())
}

/// Connect every shared backend and build the process-wide [`Services`].
pub async fn services(cfg: Config) -> Result<Data> {
    let db = connect_db(&cfg.database_url).await?;
    lumi_db::migrate(&db).await?;
    let db_read = if let Some(url) = &cfg.database_read_url
        && !url.trim().is_empty()
    {
        connect_db(url).await?
    } else {
        db.clone()
    };

    let cache = Arc::new(lumi_kernel::Cache::connect(&cfg.redis_url).await?);
    let permits = Arc::new(lumi_kernel::PermitResolver::load(&db));
    let security = Arc::new(lumi_kernel::SecurityManager::new(db.clone()));
    let downloader = Arc::new(lumi_kernel::Downloader::new(
        db.clone(),
        std::path::PathBuf::from(&cfg.custom_dir),
    ));

    let services: Data = Arc::new(Services {
        cfg,
        db,
        db_read,
        cache: Arc::clone(&cache),
        permits,
        registry: registry(),
        security,
        downloader,
    });

    tokio::spawn(async move { cache.spawn_invalidation().await });

    spawn_backend_probe(&services);

    Ok(services)
}

/// Wrap user data for [`serenity::ClientBuilder::data`].
#[must_use]
pub fn client_data(data: Data) -> Arc<Data> {
    Arc::new(data)
}

/// Resolve once Ctrl-C or SIGTERM arrives.
pub async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            tracing::error!(error = %err, "failed to register ctrl_c handler");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(err) => {
                tracing::error!(error = %err, "failed to register sigterm handler");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_data_keeps_poise_user_data_shape() {
        let _: fn(Data) -> Arc<Data> = client_data;
    }

    #[test]
    fn registry_is_non_empty_and_unique() {
        let registry = registry();
        let names: Vec<_> = registry.iter().map(lumi_kernel::Module::name).collect();
        let mut unique = names.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(names.len(), unique.len(), "duplicate module registered");
        assert!(!registry.is_empty());
    }
}
