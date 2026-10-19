pub mod entities;
pub mod migrations;

use sea_orm::entity::prelude::ChronoUtc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Database, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
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

/// Set a guild's command prefix, creating the guild row when it does not exist.
pub async fn set_prefix(db: &DatabaseConnection, guild_id: u64, prefix: &str) -> Result<()> {
    let id = guild_id.to_string();
    let now = ChronoUtc::now().fixed_offset();

    match entities::guild::Entity::find_by_id(id.clone()).one(db).await? {
        Some(model) => {
            let mut active: entities::guild::ActiveModel = model.into();
            active.prefix = Set(Some(prefix.to_string()));
            active.updated_at = Set(now);
            active.update(db).await?;
        }
        None => {
            let active = entities::guild::ActiveModel {
                id: Set(id),
                prefix: Set(Some(prefix.to_string())),
                locale: Set("en-US".to_string()),
                ignored: Set(false),
                left_at: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
            };
            active.insert(db).await?;
        }
    }
    Ok(())
}

/// Clear a guild's prefix override so it falls back to the global default.
pub async fn reset_prefix(db: &DatabaseConnection, guild_id: u64) -> Result<()> {
    if let Some(model) = entities::guild::Entity::find_by_id(guild_id.to_string())
        .one(db)
        .await?
    {
        let mut active: entities::guild::ActiveModel = model.into();
        active.prefix = Set(None);
        active.updated_at = Set(ChronoUtc::now().fixed_offset());
        active.update(db).await?;
    }
    Ok(())
}

/// How many modules a guild has explicitly enabled.
pub async fn count_enabled_modules(db: &DatabaseConnection, guild_id: u64) -> u64 {
    entities::guild_module_state::Entity::find()
        .filter(entities::guild_module_state::Column::GuildId.eq(guild_id.to_string()))
        .filter(entities::guild_module_state::Column::Enabled.eq(true))
        .count(db)
        .await
        .unwrap_or(0)
}

/// A guild's permits ordered by their display position.
pub async fn list_permits(
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<Vec<entities::permit::Model>> {
    entities::permit::Entity::find()
        .filter(entities::permit::Column::GuildId.eq(guild_id.to_string()))
        .order_by_asc(entities::permit::Column::Position)
        .all(db)
        .await
}

/// Delete a permit by id; returns how many rows were removed.
pub async fn delete_permit(db: &DatabaseConnection, id: i32) -> Result<u64> {
    let result = entities::permit::Entity::delete_by_id(id).exec(db).await?;
    Ok(result.rows_affected)
}

/// Move a permit one step up/down within its guild by re-normalising `position`.
pub async fn move_permit(
    db: &DatabaseConnection,
    guild_id: u64,
    permit_id: i32,
    direction: &str,
) -> Result<()> {
    let mut permits = list_permits(db, guild_id).await?;
    let Some(index) = permits.iter().position(|p| p.id == permit_id) else {
        return Ok(());
    };
    let target = match direction {
        "up" if index > 0 => index - 1,
        "down" if index + 1 < permits.len() => index + 1,
        _ => return Ok(()),
    };
    permits.swap(index, target);

    let now = ChronoUtc::now().fixed_offset();
    for (position, permit) in permits.iter().enumerate() {
        let position = position as i32;
        if permit.position != position {
            let mut active: entities::permit::ActiveModel = permit.clone().into();
            active.position = Set(position);
            active.updated_at = Set(now);
            active.update(db).await?;
        }
    }
    Ok(())
}
