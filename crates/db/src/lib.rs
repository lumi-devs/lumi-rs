pub mod entities;
pub mod migrations;

use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, QueryFilter,
    QuerySelect, Set, TransactionTrait,
};
use sea_orm_migration::MigratorTrait;

use crate::entities::counter;
use crate::migrations::Migrator;

pub type Result<T> = std::result::Result<T, sea_orm::DbErr>;

pub async fn connect(url: &str) -> Result<DatabaseConnection> {
    Database::connect(url).await
}

pub async fn migrate(db: &DatabaseConnection) -> Result<()> {
    Migrator::up(db, None).await?;
    Ok(())
}

/// Atomically increments and returns the next value of a named per-guild
/// counter (e.g. a moderation case number). Implemented with the ORM inside a
/// `FOR UPDATE` transaction — modules never write SQL themselves.
pub async fn next_counter(db: &DatabaseConnection, guild_id: i64, name: &str) -> Result<i64> {
    let txn = db.begin().await?;
    let existing = counter::Entity::find()
        .filter(counter::Column::GuildId.eq(guild_id))
        .filter(counter::Column::Name.eq(name))
        .lock_exclusive()
        .one(&txn)
        .await?;

    let value = match existing {
        Some(model) => {
            let next = model.value + 1;
            let mut active: counter::ActiveModel = model.into();
            active.value = Set(next);
            active.update(&txn).await?;
            next
        }
        None => {
            let active = counter::ActiveModel {
                guild_id: Set(guild_id),
                name: Set(name.to_string()),
                value: Set(1),
            };
            active.insert(&txn).await?;
            1
        }
    };

    txn.commit().await?;
    Ok(value)
}
