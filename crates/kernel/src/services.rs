use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::cache::Cache;
use crate::config::Config;
use crate::module::ModuleRegistry;
use crate::permits::PermitResolver;
use crate::security::SecurityManager;

/// Everything a command, listener or addon needs at runtime.
#[derive(Debug)]
pub struct Services {
    pub cfg: Config,
    pub db: DatabaseConnection,
    pub cache: Arc<Cache>,
    pub permits: Arc<PermitResolver>,
    pub registry: ModuleRegistry,
    pub security: Arc<SecurityManager>,
}
