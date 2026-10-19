use chrono::{TimeDelta, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use lumi_db::entities::{enums::CaseAction, mod_note, moderation_case, warn_threshold};
use lumi_kernel::module_state;
use lumi_kernel::Result;

use crate::config;

const CONFIG_MODULE: &str = "mod";

/// Resolved moderation settings. Loaded once per command instead of issuing a
/// query per key, and shared by punishment + logging paths.
#[derive(Clone, Debug)]
pub struct ModConfig {
    pub dm_on_action: bool,
    pub log_channel: Option<u64>,
    pub quarantine_role: Option<u64>,
}

pub async fn config(db: &DatabaseConnection, guild_id: u64) -> ModConfig {
    let map = module_state::get_config(db, guild_id, CONFIG_MODULE).await;
    ModConfig {
        dm_on_action: config::boolean(&map, "dm_on_action", true),
        log_channel: config::id(&map, "mod_log_channel"),
        quarantine_role: config::id(&map, "quarantine_role_id"),
    }
}

#[derive(Clone, Debug)]
pub struct CreateCaseParams<'a> {
    pub guild_id: u64,
    pub user_id: u64,
    pub moderator_id: u64,
    pub action: CaseAction,
    pub reason: Option<&'a str>,
    pub duration_seconds: Option<i32>,
}

/// Create a new moderation case using `db::next_counter` to ensure continuous
/// per-guild case numbering without race conditions or raw SQL.
pub async fn create_case(
    db: &DatabaseConnection,
    params: CreateCaseParams<'_>,
) -> Result<moderation_case::Model> {
    let case_num = lumi_db::next_counter(db, params.guild_id, "cases").await?;
    let now = Utc::now();
    let expires_at = params
        .duration_seconds
        .and_then(|seconds| TimeDelta::try_seconds(i64::from(seconds)))
        .and_then(|delta| now.checked_add_signed(delta))
        .map(|at| at.fixed_offset());

    let row = moderation_case::ActiveModel {
        guild_id: Set(lumi_db::ids::GuildId::from(params.guild_id)),
        case_number: Set(case_num as i32),
        user_id: Set(lumi_db::ids::UserId::from(params.user_id)),
        moderator_id: Set(lumi_db::ids::UserId::from(params.moderator_id)),
        action: Set(params.action),
        custom_action: Set(None),
        reason: Set(params.reason.map(ToString::to_string)),
        duration: Set(params.duration_seconds),
        expires_at: Set(expires_at),
        active: Set(true),
        created_at: Set(now.fixed_offset()),
        ..Default::default()
    };

    let model = row.insert(db).await?;
    Ok(model)
}

pub async fn get_case(
    db: &DatabaseConnection,
    guild_id: u64,
    case_number: i32,
) -> Result<Option<moderation_case::Model>> {
    Ok(moderation_case::Entity::find()
        .filter(moderation_case::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .filter(moderation_case::Column::CaseNumber.eq(case_number))
        .one(db)
        .await?)
}

pub async fn update_case_reason(
    db: &DatabaseConnection,
    guild_id: u64,
    case_number: i32,
    new_reason: &str,
) -> Result<bool> {
    let result = moderation_case::Entity::update_many()
        .col_expr(
            moderation_case::Column::Reason,
            sea_orm::sea_query::Expr::value(Some(new_reason.to_string())),
        )
        .filter(moderation_case::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .filter(moderation_case::Column::CaseNumber.eq(case_number))
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}

pub async fn delete_case(
    db: &DatabaseConnection,
    guild_id: u64,
    case_number: i32,
) -> Result<Option<moderation_case::Model>> {
    let case = get_case(db, guild_id, case_number).await?;
    if let Some(case) = &case {
        moderation_case::Entity::delete_by_id(case.id).exec(db).await?;
    }
    Ok(case)
}

pub async fn list_cases(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: Option<u64>,
    action: Option<CaseAction>,
    limit: u64,
) -> Result<Vec<moderation_case::Model>> {
    let mut query = moderation_case::Entity::find()
        .filter(moderation_case::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)));

    if let Some(uid) = user_id {
        query = query.filter(moderation_case::Column::UserId.eq(lumi_db::ids::UserId::from(uid)));
    }
    if let Some(act) = action {
        query = query.filter(moderation_case::Column::Action.eq(act));
    }

    Ok(query
        .order_by_desc(moderation_case::Column::CaseNumber)
        .limit(limit)
        .all(db)
        .await?)
}

pub async fn count_warns(db: &DatabaseConnection, guild_id: u64, user_id: u64) -> Result<u64> {
    Ok(moderation_case::Entity::find()
        .filter(moderation_case::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .filter(moderation_case::Column::UserId.eq(lumi_db::ids::UserId::from(user_id)))
        .filter(moderation_case::Column::Action.eq(CaseAction::Warn))
        .filter(moderation_case::Column::Active.eq(true))
        .count(db)
        .await?)
}

pub async fn get_warn_thresholds(
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<Vec<warn_threshold::Model>> {
    Ok(warn_threshold::Entity::find()
        .filter(warn_threshold::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .order_by_asc(warn_threshold::Column::WarnCount)
        .all(db)
        .await?)
}

pub async fn add_note(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    author_id: u64,
    message: &str,
) -> Result<mod_note::Model> {
    let row = mod_note::ActiveModel {
        guild_id: Set(lumi_db::ids::GuildId::from(guild_id)),
        user_id: Set(lumi_db::ids::UserId::from(user_id)),
        author_id: Set(lumi_db::ids::UserId::from(author_id)),
        message: Set(message.to_string()),
        created_at: Set(Utc::now().fixed_offset()),
        ..Default::default()
    };
    Ok(row.insert(db).await?)
}

pub async fn list_notes(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
) -> Result<Vec<mod_note::Model>> {
    Ok(mod_note::Entity::find()
        .filter(mod_note::Column::GuildId.eq(lumi_db::ids::GuildId::from(guild_id)))
        .filter(mod_note::Column::UserId.eq(lumi_db::ids::UserId::from(user_id)))
        .order_by_desc(mod_note::Column::CreatedAt)
        .all(db)
        .await?)
}

/// The configured quarantine role id, or `None` when quarantine is unconfigured.
pub async fn quarantine_role(db: &DatabaseConnection, guild_id: u64) -> Result<Option<u64>> {
    Ok(module_state::config_u64(db, guild_id, CONFIG_MODULE, "quarantine_role_id").await?)
}
