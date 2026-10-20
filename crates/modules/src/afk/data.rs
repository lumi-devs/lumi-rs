use chrono::Utc;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};

use lumi_db::entities::afk_entry;
use lumi_kernel::Result;

pub async fn set(db: &DatabaseConnection, guild_id: u64, user_id: u64, reason: &str) -> Result<()> {
    let row = afk_entry::ActiveModel {
        user_id: Set(lumi_db::ids::UserId::from(user_id)),
        guild_id: Set(lumi_db::ids::GuildId::from(guild_id)),
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

/// AFK rows for several users in one guild, for mention fan-out.
pub async fn get_many(
    db: &DatabaseConnection,
    guild_id: u64,
    user_ids: &[u64],
) -> Result<Vec<afk_entry::Model>> {
    if user_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<i64> = user_ids.iter().map(|id| *id as i64).collect();
    Ok(afk_entry::Entity::find()
        .filter(afk_entry::Column::GuildId.eq(guild_id as i64))
        .filter(afk_entry::Column::UserId.is_in(ids))
        .all(db)
        .await?)
}

/// Counted in the database instead of loading every row.
pub async fn count(db: &DatabaseConnection, guild_id: u64) -> Result<u64> {
    Ok(afk_entry::Entity::find()
        .filter(afk_entry::Column::GuildId.eq(guild_id as i64))
        .count(db)
        .await?)
}

/// Collapse a multi-line reason to one line, defaulting to `AFK`.
pub fn sanitize_reason(reason: &str) -> String {
    let joined = reason
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let collapsed = joined.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return "AFK".to_string();
    }
    crate::shared::truncate_chars(&collapsed, super::constants::MAX_REASON)
}

#[cfg(test)]
mod tests {
    use super::sanitize_reason;

    #[test]
    fn sanitize_reason_collapses_and_defaults() {
        assert_eq!(sanitize_reason("  out\n\n to   lunch "), "out to lunch");
        assert_eq!(sanitize_reason("\n  \n"), "AFK");
        assert_eq!(sanitize_reason(&"x".repeat(200)).len(), 100);
    }
}
