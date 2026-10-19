use chrono::Utc;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use serde_json::Value;

use lumi_db::entities::{enums::CaseAction, guild_config, mod_note, moderation_case};
use lumi_kernel::Result;

const CONFIG_MODULE: &str = "mod";

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
    let case_num = lumi_db::next_counter(db, params.guild_id as i64, "cases").await?;

    let row = moderation_case::ActiveModel {
        guild_id: Set(params.guild_id.to_string()),
        case_number: Set(case_num as i32),
        user_id: Set(params.user_id.to_string()),
        moderator_id: Set(params.moderator_id.to_string()),
        action: Set(params.action),
        custom_action: Set(None),
        reason: Set(params.reason.map(ToString::to_string)),
        duration: Set(params.duration_seconds),
        expires_at: Set(None),
        active: Set(true),
        created_at: Set(Utc::now().fixed_offset()),
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
        .filter(moderation_case::Column::GuildId.eq(guild_id.to_string()))
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
    let case_opt = get_case(db, guild_id, case_number).await?;
    if let Some(c) = case_opt {
        let mut active: moderation_case::ActiveModel = c.into();
        active.reason = Set(Some(new_reason.to_string()));
        active.update(db).await?;
        Ok(true)
    } else {
        Ok(false)
    }
}

pub async fn list_user_cases(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    limit: u64,
) -> Result<Vec<moderation_case::Model>> {
    Ok(moderation_case::Entity::find()
        .filter(moderation_case::Column::GuildId.eq(guild_id.to_string()))
        .filter(moderation_case::Column::UserId.eq(user_id.to_string()))
        .order_by_desc(moderation_case::Column::CaseNumber)
        .limit(limit)
        .all(db)
        .await?)
}

pub async fn count_warns(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
) -> Result<u64> {
    let cases = moderation_case::Entity::find()
        .filter(moderation_case::Column::GuildId.eq(guild_id.to_string()))
        .filter(moderation_case::Column::UserId.eq(user_id.to_string()))
        .filter(moderation_case::Column::Action.eq(CaseAction::Warn))
        .filter(moderation_case::Column::Active.eq(true))
        .all(db)
        .await?;
    Ok(cases.len() as u64)
}

pub async fn add_note(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    author_id: u64,
    message: &str,
) -> Result<mod_note::Model> {
    let row = mod_note::ActiveModel {
        guild_id: Set(guild_id.to_string()),
        user_id: Set(user_id.to_string()),
        author_id: Set(author_id.to_string()),
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
        .filter(mod_note::Column::GuildId.eq(guild_id.to_string()))
        .filter(mod_note::Column::UserId.eq(user_id.to_string()))
        .order_by_desc(mod_note::Column::CreatedAt)
        .all(db)
        .await?)
}

async fn get_config(
    db: &DatabaseConnection,
    guild_id: u64,
    key: &str,
) -> Result<Option<Value>> {
    Ok(guild_config::Entity::find()
        .filter(guild_config::Column::GuildId.eq(guild_id as i64))
        .filter(guild_config::Column::Module.eq(CONFIG_MODULE))
        .filter(guild_config::Column::Key.eq(key))
        .one(db)
        .await?
        .map(|model| model.value))
}

/// The configured quarantine role id, or `None` when quarantine is unconfigured.
pub async fn quarantine_role(
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<Option<u64>> {
    Ok(get_config(db, guild_id, "quarantine_role_id")
        .await?
        .and_then(|v| v.as_u64()))
}
