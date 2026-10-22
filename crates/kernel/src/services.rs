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
    /// Kernel configuration.
    pub cfg: Config,
    /// Primary database connection.
    pub db: DatabaseConnection,
    /// Read replica database connection.
    pub db_read: DatabaseConnection,
    /// In-process and distributed cache manager.
    pub cache: Arc<Cache>,
    /// Permission resolver.
    pub permits: Arc<PermitResolver>,
    /// Module registry.
    pub registry: ModuleRegistry,
    /// Anti-nuke security manager.
    pub security: Arc<SecurityManager>,
    /// Addon downloader and updater.
    pub downloader: Arc<Downloader>,
}

impl Services {
    /// The connection lag-tolerant, absolute-only reads should use.
    pub const fn reader(&self) -> &DatabaseConnection {
        &self.db_read
    }

    /// Re-read one guild's permits and assignments after they were edited, so
    /// the change is enforced immediately instead of after a process restart.
    pub async fn reload_guild_permits(&self, guild_id: u64) -> Result<()> {
        self.permits.reload_guild(&self.db, guild_id).await
    }

    /// Build every shared runtime service from `cfg`. `registry` carries the
    /// modules the caller has registered; migration is left to the caller. The
    /// cross-process cache invalidation listener is spawned here.
    pub async fn bootstrap(cfg: Config, registry: ModuleRegistry) -> Result<Arc<Self>> {
        let db_url = cfg.database_url.as_str();
        let db = retry_transient(3, Duration::from_millis(250), || async move {
            lumi_db::connect(db_url)
                .await
                .map_err(crate::LumiError::from)
        })
        .await?;
        let db_read = match &cfg.database_read_url {
            Some(url) if !url.trim().is_empty() => {
                let read_url = url.as_str();
                retry_transient(3, Duration::from_millis(250), || async move {
                    lumi_db::connect_read(read_url)
                        .await
                        .map_err(crate::LumiError::from)
                })
                .await?
            }
            _ => db.clone(),
        };
        let cache = Arc::new(Cache::connect(&cfg.redis_url).await?);
        let permits = Arc::new(PermitResolver::load(&db));
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
