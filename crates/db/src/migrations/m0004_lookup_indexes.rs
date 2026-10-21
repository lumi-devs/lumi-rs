use sea_orm_migration::prelude::*;

use crate::entities::{mod_note, permit_assignment, temp_vc_record};

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

/// Indexes for per-guild lookups that were scanning their table: permit
/// assignments by guild, mod notes by member, and a member's temp VC.
#[sea_orm_migration::async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
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
        Ok(())
    }
}
