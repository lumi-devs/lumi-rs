use sea_orm_migration::prelude::*;

use crate::entities;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Names of every index created in `up`, kept for the `down` teardown.
const INDEX_NAMES: &[&str] = &[
    // composite unique constraints
    "uq_permits_guild_id_name",
    "uq_permit_assignments_permit_target",
    "uq_moderation_cases_guild_case",
    "uq_blocklist_user_guild",
    "uq_allowlist_user_guild",
    "uq_feature_flag_overrides_flag_guild",
    "uq_ignore_list_guild_channel",
    "uq_module_config_overrides_scope",
    // moderation_cases
    "idx_moderation_cases_guild_active",
    "idx_moderation_cases_guild_user",
    "idx_moderation_cases_guild_action",
    "idx_moderation_cases_expires_at",
    "idx_moderation_cases_user_id",
    // appeals
    "idx_appeals_guild_status",
    "idx_appeals_status_created",
    "idx_appeals_user_id",
    // guilds
    "idx_guilds_left_at",
    // level_accounts
    "idx_level_accounts_guild_level",
    "idx_level_accounts_guild_xp",
    // economy_transactions
    "idx_economy_transactions_guild_user",
    "idx_economy_transactions_guild_created",
    // audit_ledger
    "idx_audit_ledger_guild_created",
    // module_config_history
    "idx_module_config_history_guild_module_created",
    "idx_module_config_history_actor_id",
    // gdpr_export_jobs
    "idx_gdpr_export_jobs_status",
    // single-column lookups
    "idx_blocklist_user_id",
    "idx_allowlist_user_id",
];

#[async_trait::async_trait]
#[allow(elided_lifetimes_in_paths)]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // composite unique constraints
        manager
            .create_index(
                Index::create()
                    .name("uq_permits_guild_id_name")
                    .table(entities::permit::Entity)
                    .col(entities::permit::Column::GuildId)
                    .col(entities::permit::Column::Name)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_permit_assignments_permit_target")
                    .table(entities::permit_assignment::Entity)
                    .col(entities::permit_assignment::Column::PermitId)
                    .col(entities::permit_assignment::Column::TargetType)
                    .col(entities::permit_assignment::Column::TargetId)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_moderation_cases_guild_case")
                    .table(entities::moderation_case::Entity)
                    .col(entities::moderation_case::Column::GuildId)
                    .col(entities::moderation_case::Column::CaseNumber)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_blocklist_user_guild")
                    .table(entities::blocklist::Entity)
                    .col(entities::blocklist::Column::UserId)
                    .col(entities::blocklist::Column::GuildId)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_allowlist_user_guild")
                    .table(entities::allowlist::Entity)
                    .col(entities::allowlist::Column::UserId)
                    .col(entities::allowlist::Column::GuildId)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_feature_flag_overrides_flag_guild")
                    .table(entities::feature_flag_override::Entity)
                    .col(entities::feature_flag_override::Column::FlagKey)
                    .col(entities::feature_flag_override::Column::GuildId)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_ignore_list_guild_channel")
                    .table(entities::ignore_entry::Entity)
                    .col(entities::ignore_entry::Column::GuildId)
                    .col(entities::ignore_entry::Column::ChannelId)
                    .unique()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_module_config_overrides_scope")
                    .table(entities::module_config_override::Entity)
                    .col(entities::module_config_override::Column::GuildId)
                    .col(entities::module_config_override::Column::ModuleName)
                    .col(entities::module_config_override::Column::Key)
                    .col(entities::module_config_override::Column::ModelType)
                    .col(entities::module_config_override::Column::ModelId)
                    .unique()
                    .to_owned(),
            )
            .await?;

        // moderation_cases
        manager
            .create_index(
                Index::create()
                    .name("idx_moderation_cases_guild_active")
                    .table(entities::moderation_case::Entity)
                    .col(entities::moderation_case::Column::GuildId)
                    .col(entities::moderation_case::Column::Active)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_moderation_cases_guild_user")
                    .table(entities::moderation_case::Entity)
                    .col(entities::moderation_case::Column::GuildId)
                    .col(entities::moderation_case::Column::UserId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_moderation_cases_guild_action")
                    .table(entities::moderation_case::Entity)
                    .col(entities::moderation_case::Column::GuildId)
                    .col(entities::moderation_case::Column::Action)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_moderation_cases_expires_at")
                    .table(entities::moderation_case::Entity)
                    .col(entities::moderation_case::Column::ExpiresAt)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_moderation_cases_user_id")
                    .table(entities::moderation_case::Entity)
                    .col(entities::moderation_case::Column::UserId)
                    .to_owned(),
            )
            .await?;

        // appeals
        manager
            .create_index(
                Index::create()
                    .name("idx_appeals_guild_status")
                    .table(entities::appeal::Entity)
                    .col(entities::appeal::Column::GuildId)
                    .col(entities::appeal::Column::Status)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_appeals_status_created")
                    .table(entities::appeal::Entity)
                    .col(entities::appeal::Column::Status)
                    .col(entities::appeal::Column::CreatedAt)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_appeals_user_id")
                    .table(entities::appeal::Entity)
                    .col(entities::appeal::Column::UserId)
                    .to_owned(),
            )
            .await?;

        // guilds
        manager
            .create_index(
                Index::create()
                    .name("idx_guilds_left_at")
                    .table(entities::guild::Entity)
                    .col(entities::guild::Column::LeftAt)
                    .to_owned(),
            )
            .await?;

        // level_accounts
        manager
            .create_index(
                Index::create()
                    .name("idx_level_accounts_guild_level")
                    .table(entities::level_account::Entity)
                    .col(entities::level_account::Column::GuildId)
                    .col(entities::level_account::Column::Level)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_level_accounts_guild_xp")
                    .table(entities::level_account::Entity)
                    .col(entities::level_account::Column::GuildId)
                    .col(entities::level_account::Column::Xp)
                    .to_owned(),
            )
            .await?;

        // economy_transactions
        manager
            .create_index(
                Index::create()
                    .name("idx_economy_transactions_guild_user")
                    .table(entities::economy_transaction::Entity)
                    .col(entities::economy_transaction::Column::GuildId)
                    .col(entities::economy_transaction::Column::UserId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_economy_transactions_guild_created")
                    .table(entities::economy_transaction::Entity)
                    .col(entities::economy_transaction::Column::GuildId)
                    .col(entities::economy_transaction::Column::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // audit_ledger
        manager
            .create_index(
                Index::create()
                    .name("idx_audit_ledger_guild_created")
                    .table(entities::audit_ledger::Entity)
                    .col(entities::audit_ledger::Column::GuildId)
                    .col(entities::audit_ledger::Column::CreatedAt)
                    .to_owned(),
            )
            .await?;

        // module_config_history
        manager
            .create_index(
                Index::create()
                    .name("idx_module_config_history_guild_module_created")
                    .table(entities::module_config_history::Entity)
                    .col(entities::module_config_history::Column::GuildId)
                    .col(entities::module_config_history::Column::ModuleName)
                    .col(entities::module_config_history::Column::CreatedAt)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_module_config_history_actor_id")
                    .table(entities::module_config_history::Entity)
                    .col(entities::module_config_history::Column::ActorId)
                    .to_owned(),
            )
            .await?;

        // gdpr_export_jobs
        manager
            .create_index(
                Index::create()
                    .name("idx_gdpr_export_jobs_status")
                    .table(entities::gdpr_export_job::Entity)
                    .col(entities::gdpr_export_job::Column::Status)
                    .to_owned(),
            )
            .await?;

        // single-column lookups
        manager
            .create_index(
                Index::create()
                    .name("idx_blocklist_user_id")
                    .table(entities::blocklist::Entity)
                    .col(entities::blocklist::Column::UserId)
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_allowlist_user_id")
                    .table(entities::allowlist::Entity)
                    .col(entities::allowlist::Column::UserId)
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for name in INDEX_NAMES {
            manager
                .drop_index(Index::drop().name(*name).to_owned())
                .await?;
        }
        Ok(())
    }
}
