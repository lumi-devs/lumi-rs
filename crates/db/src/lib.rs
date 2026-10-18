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

/// Look up the prefix for a guild, falling back to global_config.default_prefix, then ",".
pub async fn get_prefix(db: &DatabaseConnection, guild_id: Option<u64>) -> String {
    if let Some(gid) = guild_id {
        if let Ok(Some(guild)) = entities::guild::Entity::find_by_id(gid.to_string())
            .one(db)
            .await
        {
            if let Some(prefix) = guild.prefix {
                if !prefix.is_empty() {
                    return prefix;
                }
            }
        }
    }

    if let Ok(Some(global)) = entities::global::Entity::find().one(db).await {
        if !global.default_prefix.is_empty() {
            return global.default_prefix;
        }
    }

    ",".to_string()
}
