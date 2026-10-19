use chrono::Utc;
use sea_orm::entity::prelude::DateTimeWithTimeZone;
use sea_orm::sea_query::OnConflict;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};

use lumi_db::entities::{level_account, level_xp_gain};
use lumi_kernel::{LumiError, Result};

pub const START_LEVEL: i32 = 1;
pub const MAX_LEVEL: i32 = 1000;
pub const DEFAULT_MESSAGE_COOLDOWN_SECS: i64 = 60;

/// What `/level add|remove` adjusts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XpKind {
    Xp,
    Level,
}

impl XpKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "xp" => Some(Self::Xp),
            "level" => Some(Self::Level),
            _ => None,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Xp => "XP",
            Self::Level => "Levels",
        }
    }
}

#[must_use]
pub fn xp_for_level(level: i32) -> i64 {
    let lv = level.max(1);
    let t = f64::from(lv - 1);
    let val = 100.0 + 0.04 * t * t * t + 0.8 * t * t + 2.0 * t + 0.5;
    val.floor() as i64
}

#[must_use]
pub fn total_xp_for_level(level: i32) -> i64 {
    (1..level.min(MAX_LEVEL))
        .fold(0i64, |total, lv| total.saturating_add(xp_for_level(lv)))
}

#[must_use]
pub fn calculate_level_up(level: i32, xp: i64) -> (i32, i64, i32) {
    let mut lv = level.clamp(START_LEVEL, MAX_LEVEL);
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
        .filter(level_account::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .filter(level_account::Column::UserId.eq(lumi_db::ids::UserId::from(user_id)))
        .one(db)
        .await?)
}

/// Find or create the account. The insert is conflict-safe for concurrent first
/// messages from the same member.
pub async fn get_or_create_account<C: ConnectionTrait>(
    db: &C,
    guild_id: u64,
    user_id: u64,
) -> Result<level_account::Model> {
    if let Some(acc) = get_account(db, guild_id, user_id).await? {
        return Ok(acc);
    }

    let row = level_account::ActiveModel {
        guild_id: Set(lumi_db::ids::GuildId::from(guild_id)),
        user_id: Set(lumi_db::ids::UserId::from(user_id)),
        level: Set(START_LEVEL),
        xp: Set(0),
        total_xp: Set(0),
        message_count: Set(0),
        voice_minutes: Set(0),
        last_xp_at: Set(None),
    };
    level_account::Entity::insert(row)
        .on_conflict(
            OnConflict::columns([
                level_account::Column::GuildId,
                level_account::Column::UserId,
            ])
            .do_nothing()
            .to_owned(),
        )
        .exec(db)
        .await?;

    get_account(db, guild_id, user_id)
        .await?
        .ok_or_else(|| LumiError::Other("level account insert was lost".to_string()))
}

/// Account row locked for the duration of a mutation transaction.
async fn locked_account<C: ConnectionTrait>(
    db: &C,
    guild_id: u64,
    user_id: u64,
) -> Result<level_account::Model> {
    get_or_create_account(db, guild_id, user_id).await?;
    level_account::Entity::find()
        .filter(level_account::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .filter(level_account::Column::UserId.eq(lumi_db::ids::UserId::from(user_id)))
        .lock_exclusive()
        .one(db)
        .await?
        .ok_or_else(|| LumiError::Other("level account disappeared".to_string()))
}

async fn log_gain<C: ConnectionTrait>(
    db: &C,
    guild_id: u64,
    user_id: u64,
    amount: i64,
    source: &str,
    now: DateTimeWithTimeZone,
) -> Result<()> {
    level_xp_gain::ActiveModel {
        guild_id: Set(lumi_db::ids::GuildId::from(guild_id)),
        user_id: Set(lumi_db::ids::UserId::from(user_id)),
        amount: Set(amount),
        source: Set(source.to_string()),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(db)
    .await?;
    Ok(())
}

pub async fn award_xp(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    amount: i64,
    cooldown_seconds: i64,
) -> Result<Option<(level_account::Model, i32)>> {
    let txn = db.begin().await?;
    let account = locked_account(&txn, guild_id, user_id).await?;

    let now = Utc::now().fixed_offset();
    if is_on_cooldown(account.last_xp_at, cooldown_seconds) {
        return Ok(None);
    }

    let progress = account.xp.saturating_add(amount);
    let (new_level, new_xp, gained) = calculate_level_up(account.level, progress);
    let new_total_xp = account.total_xp.saturating_add(amount.max(0));
    let message_count = account.message_count.saturating_add(1);

    let mut active: level_account::ActiveModel = account.into();
    active.level = Set(new_level);
    active.xp = Set(new_xp);
    active.total_xp = Set(new_total_xp);
    active.message_count = Set(message_count);
    active.last_xp_at = Set(Some(now));
    let updated = active.update(&txn).await?;

    if amount > 0 {
        log_gain(&txn, guild_id, user_id, amount, "text", now).await?;
    }

    txn.commit().await?;
    Ok(Some((updated, gained)))
}

pub async fn add_voice_minutes(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    minutes: i32,
    xp: i64,
) -> Result<level_account::Model> {
    let txn = db.begin().await?;
    let account = locked_account(&txn, guild_id, user_id).await?;

    let now = Utc::now().fixed_offset();
    let new_total_xp = account.total_xp.saturating_add(xp.max(0));
    let (new_level, new_xp, _) =
        calculate_level_up(account.level, account.xp.saturating_add(xp));
    let voice_minutes = account.voice_minutes.saturating_add(minutes);

    let mut active: level_account::ActiveModel = account.into();
    active.level = Set(new_level);
    active.xp = Set(new_xp);
    active.total_xp = Set(new_total_xp);
    active.voice_minutes = Set(voice_minutes);
    let updated = active.update(&txn).await?;

    if xp > 0 {
        log_gain(&txn, guild_id, user_id, xp, "voice", now).await?;
    }

    txn.commit().await?;
    Ok(updated)
}

pub async fn set_level_and_xp(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    level: i32,
    xp: Option<i64>,
) -> Result<level_account::Model> {
    let level = level.clamp(START_LEVEL, MAX_LEVEL);
    let max_xp = (xp_for_level(level) - 1).max(0);
    let xp = xp.unwrap_or(0).clamp(0, max_xp);
    let total_xp = total_xp_for_level(level).saturating_add(xp);

    let txn = db.begin().await?;
    let account = locked_account(&txn, guild_id, user_id).await?;

    let mut active: level_account::ActiveModel = account.into();
    active.level = Set(level);
    active.xp = Set(xp);
    active.total_xp = Set(total_xp);
    let updated = active.update(&txn).await?;
    txn.commit().await?;
    Ok(updated)
}

pub async fn add_xp_or_level(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    kind: XpKind,
    amount: i64,
) -> Result<level_account::Model> {
    let delta = i32::try_from(amount).unwrap_or(i32::MAX);

    let txn = db.begin().await?;
    let account = locked_account(&txn, guild_id, user_id).await?;

    let mut active: level_account::ActiveModel = account.clone().into();
    match kind {
        XpKind::Level => {
            let new_level = account.level.saturating_add(delta).min(MAX_LEVEL);
            active.level = Set(new_level);
            active.total_xp = Set(total_xp_for_level(new_level).saturating_add(account.xp));
        }
        XpKind::Xp => {
            let (new_level, new_xp, _) =
                calculate_level_up(account.level, account.xp.saturating_add(amount));
            active.level = Set(new_level);
            active.xp = Set(new_xp);
            active.total_xp = Set(total_xp_for_level(new_level).saturating_add(new_xp));
        }
    }
    let updated = active.update(&txn).await?;
    txn.commit().await?;
    Ok(updated)
}

pub async fn remove_xp_or_level(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    kind: XpKind,
    amount: i64,
) -> Result<level_account::Model> {
    let delta = i32::try_from(amount).unwrap_or(i32::MAX);

    let txn = db.begin().await?;
    let account = locked_account(&txn, guild_id, user_id).await?;

    let mut active: level_account::ActiveModel = account.clone().into();
    match kind {
        XpKind::Level => {
            let new_level = account.level.saturating_sub(delta).max(START_LEVEL);
            active.level = Set(new_level);
            active.total_xp = Set(total_xp_for_level(new_level).saturating_add(account.xp));
        }
        XpKind::Xp => {
            let before = total_xp_for_level(account.level).saturating_add(account.xp);
            let total = before.saturating_sub(amount).max(0);
            let mut level = account.level;
            while level > START_LEVEL && total < total_xp_for_level(level) {
                level -= 1;
            }
            let xp = total.saturating_sub(total_xp_for_level(level));
            active.level = Set(level);
            active.xp = Set(xp);
            active.total_xp = Set(total);
        }
    }
    let updated = active.update(&txn).await?;
    txn.commit().await?;
    Ok(updated)
}

pub async fn get_user_rank(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
) -> Result<Option<i64>> {
    let Some(acc) = get_account(db, guild_id, user_id).await? else {
        return Ok(None);
    };

    let count = level_account::Entity::find()
        .filter(level_account::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .filter(level_account::Column::TotalXp.gt(acc.total_xp))
        .count(db)
        .await?;

    Ok(Some(i64::try_from(count).unwrap_or(i64::MAX).saturating_add(1)))
}

pub async fn get_leaderboard(
    db: &DatabaseConnection,
    guild_id: u64,
    limit: u64,
    offset: u64,
) -> Result<Vec<level_account::Model>> {
    Ok(level_account::Entity::find()
        .filter(level_account::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .order_by_desc(level_account::Column::TotalXp)
        .order_by_desc(level_account::Column::Level)
        .order_by_desc(level_account::Column::Xp)
        .order_by_asc(level_account::Column::UserId)
        .limit(limit)
        .offset(offset)
        .all(db)
        .await?)
}

pub async fn count_accounts(db: &DatabaseConnection, guild_id: u64) -> Result<u64> {
    Ok(level_account::Entity::find()
        .filter(level_account::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .count(db)
        .await?)
}

#[cfg(test)]
mod tests {
    use super::{XpKind, calculate_level_up, total_xp_for_level, xp_for_level};

    #[test]
    fn xp_curve_matches_disgaea() {
        assert_eq!(xp_for_level(1), 100);
        assert_eq!(xp_for_level(2), 103);
        assert!(xp_for_level(10) > xp_for_level(2));
    }

    #[test]
    fn level_up_carries_progress() {
        assert_eq!(calculate_level_up(1, 99), (1, 99, 0));
        assert_eq!(calculate_level_up(1, 100), (2, 0, 1));
        assert_eq!(calculate_level_up(1, 203), (3, 0, 2));
    }

    #[test]
    fn total_xp_is_monotonic() {
        assert_eq!(total_xp_for_level(1), 0);
        assert_eq!(total_xp_for_level(2), xp_for_level(1));
    }

    #[test]
    fn parses_adjust_kind() {
        assert_eq!(XpKind::parse("xp"), Some(XpKind::Xp));
        assert_eq!(XpKind::parse("level"), Some(XpKind::Level));
        assert_eq!(XpKind::parse("nope"), None);
    }
}
