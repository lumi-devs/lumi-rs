use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Checks whether `level_accounts` and `economy_accounts` are hash-partitioned
/// by `guild_id` and adds supporting indexes. The 32-partition rewrite itself
/// must happen manually in a maintenance window; this migration does not run it.
#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM pg_class c
                    JOIN pg_namespace n ON n.oid = c.relnamespace
                    WHERE c.relname = 'level_accounts' AND n.nspname = 'public' AND c.relkind = 'p'
                ) THEN
                    RAISE NOTICE 'level_accounts is unpartitioned. Ready for maintenance-window hash partition.';
                ELSE
                    RAISE NOTICE 'level_accounts is already partitioned.';
                END IF;
            END $$;",
        ).await?;

        conn.execute_unprepared(
            "DO $$
            BEGIN
                IF NOT EXISTS (
                    SELECT 1 FROM pg_class c
                    JOIN pg_namespace n ON n.oid = c.relnamespace
                    WHERE c.relname = 'economy_accounts' AND n.nspname = 'public' AND c.relkind = 'p'
                ) THEN
                    RAISE NOTICE 'economy_accounts is unpartitioned. Ready for maintenance-window hash partition.';
                ELSE
                    RAISE NOTICE 'economy_accounts is already partitioned.';
                END IF;
            END $$;",
        ).await?;

        conn.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_level_accounts_part_guild_xp
             ON level_accounts (guild_id, xp DESC);",
        )
        .await?;

        conn.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_economy_accounts_part_guild_net
             ON economy_accounts (guild_id, (wallet + bank) DESC) WHERE economy_accounts IS NOT NULL;"
        ).await?;

        Ok(())
    }
}
