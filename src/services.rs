use std::sync::Arc;

use sea_orm::DatabaseConnection;

use crate::addon::AddonHost;
use crate::cache::Cache;
use crate::config::Config;
use crate::module::ModuleRegistry;
use crate::permits::PermitResolver;

/// Everything a command, listener or addon needs at runtime.
#[derive(Debug)]
pub struct Services {
    pub cfg: Config,
    pub db: DatabaseConnection,
    pub addons: Arc<AddonHost>,
    pub cache: Arc<Cache>,
    pub permits: Arc<PermitResolver>,
    pub registry: ModuleRegistry,
}
