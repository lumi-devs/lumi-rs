use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Drop the dead `guild_case_counter` table: superseded by the generic
/// `counters` entity. `if_exists` keeps fresh databases (where schema-sync
/// never created it) working.
#[async_trait::async_trait]
#[allow(elided_lifetimes_in_paths)]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("guild_case_counter"))
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("guild_case_counter"))
                    .col(
                        ColumnDef::new(Alias::new("guild_id"))
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Alias::new("next")).integer().not_null())
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
