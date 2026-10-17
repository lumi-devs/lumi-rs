use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Baseline schema. Entities are the source of truth: this syncs the tables
/// from the entity registry, so there is no hand-written SQL to drift.
#[async_trait::async_trait]
#[allow(elided_lifetimes_in_paths)]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .get_schema_registry("lumi::entities::*")
            .sync(manager.get_connection())
            .await?;
        Ok(())
    }
}
