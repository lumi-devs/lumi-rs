use sea_orm::{Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;

use crate::migrations::Migrator;
use crate::Result;

pub async fn connect(url: &str) -> Result<DatabaseConnection> {
    Ok(Database::connect(url).await?)
}

pub async fn migrate(db: &DatabaseConnection) -> Result<()> {
    Migrator::up(db, None).await?;
    Ok(())
}
