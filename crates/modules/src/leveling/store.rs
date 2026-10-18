use chrono::Utc;
use sea_orm::entity::prelude::DateTimeWithTimeZone;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};

use lumi_db::entities::{level_account, level_xp_gain};
use lumi_kernel::Result;

pub const START_LEVEL: i32 = 1;
pub const MAX_LEVEL: i32 = 1000;
pub const DEFAULT_MESSAGE_COOLDOWN_SECS: i64 = 60;

#[must_use]
pub fn xp_for_level(level: i32) -> i64 {
    let lv = level.max(1);
    let t = f64::from(lv - 1);
    let val = 100.0 + 0.04 * t * t * t + 0.8 * t * t + 2.0 * t + 0.5;
    val.floor() as i64
}

#[must_use]
pub fn total_xp_for_level(level: i32) -> i64 {
    let mut total = 0i64;
    for lv in 1..level.min(MAX_LEVEL) {
        total += xp_for_level(lv);
    }
    total
}

#[must_use]
pub fn calculate_level_up(level: i32, xp: i64) -> (i32, i64, i32) {
    let mut lv = level.max(START_LEVEL);
    let mut progress = xp.max(0);
    let mut gained = 0;
    while lv < MAX_LEVEL {
        let needed = xp_for_level(lv);
        if progress < needed {
            break;
        }
        progress -= needed;
        lv += 1;
        gained += 1;
    }
    (lv, progress, gained)
}

#[must_use]
pub fn is_on_cooldown(last_xp_at: Option<DateTimeWithTimeZone>, cooldown_seconds: i64) -> bool {
    let Some(last_at) = last_xp_at else {
        return false;
    };
    let now = Utc::now().fixed_offset();
    (now - last_at).num_seconds() < cooldown_seconds
}

pub async fn get_account<C: ConnectionTrait>(
    db: &C,
    guild_id: u64,
    user_id: u64,
) -> Result<Option<level_account::Model>> {
    Ok(level_account::Entity::find()
        .filter(level_account::Column::GuildId.eq(guild_id.to_string()))
        .filter(level_account::Column::UserId.eq(user_id.to_string()))
        .one(db)
        .await?)
}

pub async fn get_or_create_account<C: ConnectionTrait>(
    db: &C,
    guild_id: u64,
    user_id: u64,
) -> Result<level_account::Model> {
    if let Some(acc) = get_account(db, guild_id, user_id).await? {
        return Ok(acc);
    }
    let active = level_account::ActiveModel {
        guild_id: Set(guild_id.to_string()),
        user_id: Set(user_id.to_string()),
        level: Set(START_LEVEL),
        xp: Set(0),
        total_xp: Set(0),
        message_count: Set(0),
        voice_minutes: Set(0),
        last_xp_at: Set(None),
    };
    Ok(active.insert(db).await?)
}

pub async fn award_xp(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    amount: i64,
    cooldown_seconds: i64,
) -> Result<Option<(level_account::Model, i32)>> {
    let txn = db.begin().await?;
    let account = get_or_create_account(&txn, guild_id, user_id).await?;

    let now = Utc::now().fixed_offset();
    if is_on_cooldown(account.last_xp_at, cooldown_seconds) {
        return Ok(None);
    }

    let new_total_xp = account.total_xp + amount;
    let (new_level, new_xp, gained) = calculate_level_up(account.level, account.xp + amount);

    let mut active: level_account::ActiveModel = account.into();
    active.level = Set(new_level);
    active.xp = Set(new_xp);
    active.total_xp = Set(new_total_xp);
    active.message_count = Set(account.message_count + 1);
    active.last_xp_at = Set(Some(now));
    let updated = active.update(&txn).await?;

    if amount > 0 {
        let gain = level_xp_gain::ActiveModel {
            guild_id: Set(guild_id.to_string()),
            user_id: Set(user_id.to_string()),
            amount: Set(amount),
            source: Set("text".to_string()),
            created_at: Set(now),
            ..Default::default()
        };
        gain.insert(&txn).await?;
    }

    txn.commit().await?;
    Ok(Some((updated, gained)))
}

pub async fn set_level_xp(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    level: i32,
    xp: i64,
) -> Result<level_account::Model> {
    let txn = db.begin().await?;
    let account = get_or_create_account(&txn, guild_id, user_id).await?;
    let total_xp = total_xp_for_level(level) + xp;

    let mut active: level_account::ActiveModel = account.into();
    active.level = Set(level);
    active.xp = Set(xp);
    active.total_xp = Set(total_xp);
    let updated = active.update(&txn).await?;
    txn.commit().await?;
    Ok(updated)
}

pub async fn delete_account(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
) -> Result<bool> {
    let res = level_account::Entity::delete_many()
        .filter(level_account::Column::GuildId.eq(guild_id.to_string()))
        .filter(level_account::Column::UserId.eq(user_id.to_string()))
        .exec(db)
        .await?;
    Ok(res.rows_affected > 0)
}

pub async fn get_user_rank(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
) -> Result<Option<i64>> {
    let user_acc = get_account(db, guild_id, user_id).await?;
    let Some(acc) = user_acc else {
        return Ok(None);
    };

    let count = level_account::Entity::find()
        .filter(level_account::Column::GuildId.eq(guild_id.to_string()))
        .filter(level_account::Column::TotalXp.gt(acc.total_xp))
        .count(db)
        .await?;

    Ok(Some((count as i64) + 1))
}

pub async fn get_leaderboard(
    db: &DatabaseConnection,
    guild_id: u64,
    limit: u64,
) -> Result<Vec<level_account::Model>> {
    Ok(level_account::Entity::find()
        .filter(level_account::Column::GuildId.eq(guild_id.to_string()))
        .order_by_desc(level_account::Column::TotalXp)
        .order_by_desc(level_account::Column::Level)
        .order_by_desc(level_account::Column::Xp)
        .limit(limit)
        .all(db)
        .await?)
}
