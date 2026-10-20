use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Indexes for the hottest leaderboard and rank queries. `CONCURRENTLY` is
/// unavailable inside the migration transaction, so plain `CREATE INDEX` it is.
#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let connection = manager.get_connection();
        connection
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_level_guild_total \
                 ON level_accounts (guild_id, total_xp DESC, level DESC, xp DESC, user_id)",
            )
            .await?;
        connection
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_econ_guild_net \
                 ON economy_accounts (guild_id, (wallet + bank) DESC, user_id)",
            )
            .await?;
        connection
            .execute_unprepared(
                "CREATE INDEX IF NOT EXISTS idx_level_xp_guild_created \
                 ON level_xp_gains (guild_id, created_at DESC)",
            )
            .await?;
        Ok(())
    }
}
