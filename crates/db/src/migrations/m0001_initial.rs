//! Initial database schema migration.

use sea_orm_migration::prelude::*;

use crate::entities::{
    afk_entry, economy_transaction, level_account, mod_note, moderation_case, permit_assignment,
    temp_vc_record,
};

/// One composite index: name, table entity, then the columns.
macro_rules! composite {
    ($manager:expr, $name:literal, $table:expr, $($col:expr),+ $(,)?) => {
        $manager
            .create_index(
                Index::create()
                    .name($name)
                    .table($table)
                    $(.col($col))+
                    .to_owned(),
            )
            .await?
    };
}

/// Baseline schema migration.
#[derive(Debug, DeriveMigrationName)]
pub struct Migration;

#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.get_schema_registry("lumi_db::entities::*")
            .sync(conn)
            .await?;

        composite!(
            manager,
            "idx_moderation_cases_guild_active",
            moderation_case::Entity,
            moderation_case::Column::GuildId,
            moderation_case::Column::Active
        );
        composite!(
            manager,
            "idx_moderation_cases_guild_user",
            moderation_case::Entity,
            moderation_case::Column::GuildId,
            moderation_case::Column::UserId
        );
        composite!(
            manager,
            "idx_moderation_cases_guild_action",
            moderation_case::Entity,
            moderation_case::Column::GuildId,
            moderation_case::Column::Action
        );
        composite!(
            manager,
            "idx_level_accounts_guild_level",
            level_account::Entity,
            level_account::Column::GuildId,
            level_account::Column::Level
        );
        composite!(
            manager,
            "idx_level_accounts_guild_xp",
            level_account::Entity,
            level_account::Column::GuildId,
            level_account::Column::Xp
        );
        composite!(
            manager,
            "idx_economy_transactions_guild_user",
            economy_transaction::Entity,
            economy_transaction::Column::GuildId,
            economy_transaction::Column::UserId
        );
        composite!(
            manager,
            "idx_economy_transactions_guild_created",
            economy_transaction::Entity,
            economy_transaction::Column::GuildId,
            economy_transaction::Column::CreatedAt
        );
        composite!(
            manager,
            "idx_afk_entries_guild_since",
            afk_entry::Entity,
            afk_entry::Column::GuildId,
            afk_entry::Column::Since
        );

        composite!(
            manager,
            "idx_permit_assignments_guild",
            permit_assignment::Entity,
            permit_assignment::Column::GuildId
        );
        composite!(
            manager,
            "idx_mod_notes_guild_user_created",
            mod_note::Entity,
            mod_note::Column::GuildId,
            mod_note::Column::UserId,
            mod_note::Column::CreatedAt
        );
        composite!(
            manager,
            "idx_temp_vc_records_guild_owner_created",
            temp_vc_record::Entity,
            temp_vc_record::Column::GuildId,
            temp_vc_record::Column::OwnerId,
            temp_vc_record::Column::CreatedAt
        );

        conn.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_level_guild_total \
             ON level_accounts (guild_id, total_xp DESC, level DESC, xp DESC, user_id)",
        )
        .await?;
        conn.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_econ_guild_net \
             ON economy_accounts (guild_id, (wallet + bank) DESC, user_id)",
        )
        .await?;
        conn.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_level_xp_guild_created \
             ON level_xp_gains (guild_id, created_at DESC)",
        )
        .await?;
        conn.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_level_accounts_part_guild_xp \
             ON level_accounts (guild_id, xp DESC)",
        )
        .await?;
        conn.execute_unprepared(
            "CREATE INDEX IF NOT EXISTS idx_economy_accounts_part_guild_net \
             ON economy_accounts (guild_id, (wallet + bank) DESC) WHERE economy_accounts IS NOT NULL",
        )
        .await?;

        Ok(())
    }
}
