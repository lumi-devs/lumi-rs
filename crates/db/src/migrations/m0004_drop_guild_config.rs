use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Drop the dead `guild_config` table: guild module config now lives solely in
/// `guild_module_config` (the store the hub writes and `module_state` reads).
/// `if_exists` keeps fresh databases (where schema-sync never created it) working.
#[async_trait::async_trait]
#[allow(elided_lifetimes_in_paths)]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(Alias::new("guild_config"))
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
                    .table(Alias::new("guild_config"))
                    .col(
                        ColumnDef::new(Alias::new("guild_id"))
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("module"))
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("key"))
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Alias::new("value")).json_binary().not_null())
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}
