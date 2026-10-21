use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use sea_orm::DatabaseConnection;

use crate::Result;
use crate::cache::Cache;
use crate::config::Config;
use crate::downloader::Downloader;
use crate::module::ModuleRegistry;
use crate::permits::PermitResolver;
use crate::retry::retry_transient;
use crate::security::SecurityManager;

/// Everything a command, listener or addon needs at runtime.
#[derive(Debug)]
pub struct Services {
    pub cfg: Config,
    pub db: DatabaseConnection,
    pub db_read: DatabaseConnection,
    pub cache: Arc<Cache>,
    pub permits: Arc<PermitResolver>,
    pub registry: ModuleRegistry,
    pub security: Arc<SecurityManager>,
    pub downloader: Arc<Downloader>,
}

impl Services {
    /// The connection lag-tolerant, absolute-only reads should use.
    #[must_use]
    pub fn reader(&self) -> &DatabaseConnection {
        &self.db_read
    }

    /// Build every shared runtime service from `cfg`. `registry` carries the
    /// modules the caller has registered; migration is left to the caller. The
    /// cross-process cache invalidation listener is spawned here.
    pub async fn bootstrap(cfg: Config, registry: ModuleRegistry) -> Result<Arc<Self>> {
        let db_url = cfg.database_url.clone();
        let db = retry_transient(3, Duration::from_millis(250), || {
            let db_url = db_url.clone();
            async move {
                lumi_db::connect(&db_url)
                    .await
                    .map_err(crate::LumiError::from)
            }
        })
        .await?;
        let db_read = match std::env::var("DATABASE_READ_URL") {
            Ok(url) if !url.trim().is_empty() => {
                retry_transient(3, Duration::from_millis(250), || {
                    let url = url.clone();
                    async move {
                        lumi_db::connect_read(&url)
                            .await
                            .map_err(crate::LumiError::from)
                    }
                })
                .await?
            }
            _ => db.clone(),
        };
        let cache = Arc::new(Cache::connect(&cfg.redis_url).await?);
        let permits = Arc::new(PermitResolver::load(&db).await?);
        let security = Arc::new(SecurityManager::new(db.clone()));
        let downloader = Arc::new(Downloader::new(db.clone(), PathBuf::from(&cfg.custom_dir)));

        let listener = Arc::clone(&cache);
        tokio::spawn(async move { listener.spawn_invalidation().await });

        Ok(Arc::new(Self {
            cfg,
            db,
            db_read,
            cache,
            permits,
            registry,
            security,
            downloader,
        }))
    }
}
