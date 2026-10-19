use sea_orm_migration::prelude::*;

use crate::entities::{economy_transaction, level_account, moderation_case};

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

#[derive(DeriveMigrationName)]
pub struct Migration;

/// The single migration for the pre-release schema. Entities are the source of
/// truth: this schema-syncs every table, unique key and single-column index
/// declared in `lumi_db::entities::*`, then adds the composite indexes the
/// entity attributes cannot express.
#[sea_orm_migration::async_trait::async_trait]
#[allow(elided_lifetimes_in_paths)]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .get_schema_registry("lumi_db::entities::*")
            .sync(manager.get_connection())
            .await?;

        composite!(manager, "idx_moderation_cases_guild_active", moderation_case::Entity,
            moderation_case::Column::GuildId, moderation_case::Column::Active);
        composite!(manager, "idx_moderation_cases_guild_user", moderation_case::Entity,
            moderation_case::Column::GuildId, moderation_case::Column::UserId);
        composite!(manager, "idx_moderation_cases_guild_action", moderation_case::Entity,
            moderation_case::Column::GuildId, moderation_case::Column::Action);
        composite!(manager, "idx_level_accounts_guild_level", level_account::Entity,
            level_account::Column::GuildId, level_account::Column::Level);
        composite!(manager, "idx_level_accounts_guild_xp", level_account::Entity,
            level_account::Column::GuildId, level_account::Column::Xp);
        composite!(manager, "idx_economy_transactions_guild_user", economy_transaction::Entity,
            economy_transaction::Column::GuildId, economy_transaction::Column::UserId);
        composite!(manager, "idx_economy_transactions_guild_created", economy_transaction::Entity,
            economy_transaction::Column::GuildId, economy_transaction::Column::CreatedAt);
        Ok(())
    }
}
