//! Shared process wiring: module registry, service construction and supervision.
//!
//! Every entry point (gateway, API, worker) builds its [`Services`] here so the
//! module set and the poise user-data invariant live in exactly one place.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;

use lumi_kernel::bot::Data;
use lumi_kernel::{Config, ModuleRegistry, Services};

/// Postgres may still be starting when this process is; retry the initial
/// connect instead of crashing the whole tier on a cold-start race.
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
///
/// Registration lives here so the gateway, API, worker and scheduler all see
/// exactly the same module set.
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

/// How often the always-on backend latency probes run. Metrics scrapes are
/// independent of the dashboard, so these must not depend on `/ping` traffic.
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
                metrics::gauge!("lumi.db.latency_ms")
                    .set(started.elapsed().as_secs_f64() * 1000.0);
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

/// Connect every shared backend and build the process-wide [`Services`].
pub async fn services(cfg: Config) -> Result<Data> {
    let db = connect_db(&cfg.database_url).await?;
    lumi_db::migrate(&db).await?;
    let db_read = match std::env::var("DATABASE_READ_URL") {
        Ok(url) if !url.trim().is_empty() => connect_db(&url).await?,
        _ => db.clone(),
    };

    let cache = Arc::new(lumi_kernel::Cache::connect(&cfg.redis_url).await?);
    let permits = Arc::new(lumi_kernel::PermitResolver::load(&db).await?);
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

    // One subscriber per process; every process tier wants cache coherence.
    tokio::spawn(async move { cache.spawn_invalidation().await });

    spawn_backend_probe(&services);

    Ok(services)
}

/// Wrap user data for [`serenity::ClientBuilder::data`].
///
/// Poise's user-data type `U` is [`Data`] (`Arc<Services>`), and `builder.data`
/// takes `Arc<U>`, so the client must be handed `Arc<Arc<Services>>`. Routing
/// every call through this typed helper keeps `Context::data::<Data>()` and the
/// `Arc<Arc<Services>>` downcast from silently drifting apart.
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
        // Compile-time proof: `client_data` only accepts `Data` and yields
        // `Arc<Data>`, the exact type `ClientBuilder::data` and the
        // `Context::data::<Data>()` downcast require.
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
