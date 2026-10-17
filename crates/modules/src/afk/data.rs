use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set};

use lumi_db::entities::afk_entry;
use lumi_kernel::Result;

pub async fn set(db: &DatabaseConnection, guild_id: u64, user_id: u64, reason: &str) -> Result<()> {
    let row = afk_entry::ActiveModel {
        user_id: Set(user_id as i64),
        guild_id: Set(guild_id as i64),
        reason: Set(reason.to_string()),
        since: Set(Utc::now().fixed_offset()),
    };
    afk_entry::Entity::insert(row)
        .on_conflict(
            OnConflict::columns([afk_entry::Column::UserId, afk_entry::Column::GuildId])
                .update_columns([afk_entry::Column::Reason, afk_entry::Column::Since])
                .to_owned(),
        )
        .exec(db)
        .await?;
    Ok(())
}

pub async fn get(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
) -> Result<Option<afk_entry::Model>> {
    Ok(afk_entry::Entity::find()
        .filter(afk_entry::Column::GuildId.eq(guild_id as i64))
        .filter(afk_entry::Column::UserId.eq(user_id as i64))
        .one(db)
        .await?)
}

pub async fn clear(db: &DatabaseConnection, guild_id: u64, user_id: u64) -> Result<bool> {
    let result = afk_entry::Entity::delete_many()
        .filter(afk_entry::Column::GuildId.eq(guild_id as i64))
        .filter(afk_entry::Column::UserId.eq(user_id as i64))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}

pub async fn list(db: &DatabaseConnection, guild_id: u64) -> Result<Vec<afk_entry::Model>> {
    Ok(afk_entry::Entity::find()
        .filter(afk_entry::Column::GuildId.eq(guild_id as i64))
        .order_by_asc(afk_entry::Column::Since)
        .all(db)
        .await?)
}

pub async fn count(db: &DatabaseConnection, guild_id: u64) -> Result<u64> {
    let rows = afk_entry::Entity::find()
        .filter(afk_entry::Column::GuildId.eq(guild_id as i64))
        .all(db)
        .await?;
    Ok(rows.len() as u64)
}
