//! Per-guild module enable/disable state and config, backed by SeaORM.
//!
//! The Modules tab reads and writes these; module metadata and the config
//! schema stay in [`crate::module`].
//!
//! Guild config lives in exactly one table (`guild_module_config`). Modules
//! never keep their own config store: read all keys with [`get_config`], one
//! value with [`config_value`], a snowflake with [`config_u64`], and write with
//! [`set_config_field`] / [`set_config_value`].

use std::collections::HashMap;

use sea_orm::entity::prelude::ChronoUtc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde_json::{Map, Value};

use lumi_db::entities::{global_module_state, guild_module_config, guild_module_state};

use crate::module::{ConfigField, ConfigFieldKind, ModuleRegistry};

pub type Result<T> = std::result::Result<T, sea_orm::DbErr>;

/// One row of the Modules list: module metadata plus its resolved state.
#[derive(Clone, Debug)]
pub struct FeatureRow {
    pub name: String,
    pub display_name: String,
    pub emoji: String,
    pub description: String,
    pub version: String,
    pub enabled: bool,
}

/// Whether a module runs in a guild. A global row that disables the module
/// wins; otherwise the guild row wins; otherwise the module is enabled.
pub async fn is_enabled(db: &DatabaseConnection, guild_id: u64, module: &str) -> bool {
    let global = global_module_state::Entity::find()
        .filter(global_module_state::Column::ModuleName.eq(module))
        .one(db)
        .await
        .ok()
        .flatten();
    if matches!(global, Some(row) if !row.enabled) {
        return false;
    }
    guild_module_state::Entity::find()
        .filter(guild_module_state::Column::GuildId.eq(guild_id.to_string()))
        .filter(guild_module_state::Column::ModuleName.eq(module))
        .one(db)
        .await
        .ok()
        .flatten()
        .map_or(true, |row| row.enabled)
}

/// Persist a guild's enable/disable choice for a module.
pub async fn set_enabled(
    db: &DatabaseConnection,
    guild_id: u64,
    module: &str,
    enabled: bool,
) -> Result<()> {
    let gid = guild_id.to_string();
    let now = ChronoUtc::now().fixed_offset();
    match guild_module_state::Entity::find()
        .filter(guild_module_state::Column::GuildId.eq(gid.clone()))
        .filter(guild_module_state::Column::ModuleName.eq(module))
        .one(db)
        .await?
    {
        Some(row) => {
            let mut active: guild_module_state::ActiveModel = row.into();
            active.enabled = Set(enabled);
            active.updated_at = Set(now);
            active.update(db).await?;
        }
        None => {
            guild_module_state::ActiveModel {
                guild_id: Set(gid),
                module_name: Set(module.to_string()),
                enabled: Set(enabled),
                updated_at: Set(now),
            }
            .insert(db)
            .await?;
        }
    }
    Ok(())
}

/// Every registered module with its resolved enabled state, sorted by name.
pub async fn list_feature_rows(
    registry: &ModuleRegistry,
    db: &DatabaseConnection,
    guild_id: u64,
) -> Vec<FeatureRow> {
    let guild_rows: HashMap<String, bool> = guild_module_state::Entity::find()
        .filter(guild_module_state::Column::GuildId.eq(guild_id.to_string()))
        .all(db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|row| (row.module_name, row.enabled))
        .collect();
    let global_rows: HashMap<String, bool> = global_module_state::Entity::find()
        .all(db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|row| (row.module_name, row.enabled))
        .collect();

    let mut rows: Vec<FeatureRow> = registry
        .iter()
        .map(|module| {
            let meta = module.metadata();
            let name = module.name();
            let enabled = !matches!(global_rows.get(name), Some(false))
                && guild_rows.get(name).copied().unwrap_or(true);
            FeatureRow {
                name: name.to_string(),
                display_name: meta.display_name,
                emoji: meta.emoji,
                description: meta.description,
                version: meta.version,
                enabled,
            }
        })
        .collect();
    rows.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    rows
}

/// Stored per-guild overrides for a module, keyed by config key. Keys the guild
/// never set are absent; callers fall back to each field's `default`.
pub async fn get_config(
    db: &DatabaseConnection,
    guild_id: u64,
    module: &str,
) -> Map<String, Value> {
    guild_module_config::Entity::find()
        .filter(guild_module_config::Column::GuildId.eq(guild_id.to_string()))
        .filter(guild_module_config::Column::ModuleName.eq(module))
        .all(db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|row| (row.config_key, row.value))
        .collect()
}

/// A single stored config value for a guild + module, or `None` if unset.
/// Use this for one-off reads; [`get_config`] loads every key at once.
pub async fn config_value(
    db: &DatabaseConnection,
    guild_id: u64,
    module: &str,
    key: &str,
) -> Result<Option<Value>> {
    Ok(guild_module_config::Entity::find()
        .filter(guild_module_config::Column::GuildId.eq(guild_id.to_string()))
        .filter(guild_module_config::Column::ModuleName.eq(module))
        .filter(guild_module_config::Column::ConfigKey.eq(key))
        .one(db)
        .await?
        .map(|row| row.value))
}

/// Parse a JSON value as a snowflake, accepting a number or numeric string.
pub fn parse_id(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

/// A single config value parsed as a snowflake, accepting a JSON number or a
/// numeric string. `None` when unset or unparsable.
pub async fn config_u64(
    db: &DatabaseConnection,
    guild_id: u64,
    module: &str,
    key: &str,
) -> Result<Option<u64>> {
    Ok(config_value(db, guild_id, module, key)
        .await?
        .as_ref()
        .and_then(parse_id))
}

/// Upsert one config key for a guild + module.
pub async fn set_config_field(
    db: &DatabaseConnection,
    guild_id: u64,
    module: &str,
    key: &str,
    value: Value,
) -> Result<()> {
    let gid = guild_id.to_string();
    let now = ChronoUtc::now().fixed_offset();
    match guild_module_config::Entity::find()
        .filter(guild_module_config::Column::GuildId.eq(gid.clone()))
        .filter(guild_module_config::Column::ModuleName.eq(module))
        .filter(guild_module_config::Column::ConfigKey.eq(key))
        .one(db)
        .await?
    {
        Some(row) => {
            let mut active: guild_module_config::ActiveModel = row.into();
            active.value = Set(value);
            active.updated_at = Set(now);
            active.update(db).await?;
        }
        None => {
            guild_module_config::ActiveModel {
                guild_id: Set(gid),
                module_name: Set(module.to_string()),
                config_key: Set(key.to_string()),
                value: Set(value),
                updated_at: Set(now),
            }
            .insert(db)
            .await?;
        }
    }
    Ok(())
}

/// Convert a raw select value into the field's JSON type, then store it.
pub async fn set_config_value(
    db: &DatabaseConnection,
    guild_id: u64,
    module: &str,
    key: &str,
    raw: &str,
    kind: &ConfigFieldKind,
) -> Result<()> {
    let value = match kind {
        ConfigFieldKind::Boolean => Value::Bool(matches!(raw, "true" | "1" | "yes")),
        ConfigFieldKind::Integer => raw
            .parse::<i64>()
            .map_or_else(|_| Value::String(raw.to_string()), |n| Value::from(n)),
        _ => Value::String(raw.to_string()),
    };
    set_config_field(db, guild_id, module, key, value).await
}

/// Delete every stored override for a module; returns the rows removed.
pub async fn clear_config(db: &DatabaseConnection, guild_id: u64, module: &str) -> Result<u64> {
    let result = guild_module_config::Entity::delete_many()
        .filter(guild_module_config::Column::GuildId.eq(guild_id.to_string()))
        .filter(guild_module_config::Column::ModuleName.eq(module))
        .exec(db)
        .await?;
    Ok(result.rows_affected)
}

/// Render a config value the way the dashboard does (mentions, glyphs, codes).
pub fn format_field_value(field: &ConfigField, value: Option<&Value>) -> String {
    let value = value.unwrap_or(&field.default);
    if value.is_null() {
        return "-# *(not set)*".to_string();
    }
    match &field.kind {
        ConfigFieldKind::Boolean => match value.as_bool() {
            Some(true) => "✅ Yes".to_string(),
            _ => "❌ No".to_string(),
        },
        ConfigFieldKind::Channel => id_value(value, |id| format!("<#{id}>")),
        ConfigFieldKind::Role => id_value(value, |id| format!("<@&{id}>")),
        ConfigFieldKind::User => id_value(value, |id| format!("<@{id}>")),
        ConfigFieldKind::StringArray => {
            let items: Vec<String> = value
                .as_array()
                .map(|array| {
                    array
                        .iter()
                        .filter_map(|item| item.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if items.is_empty() {
                "-# *(not set)*".to_string()
            } else {
                format!("`{}`", cut(&items.join(", "), 120))
            }
        }
        ConfigFieldKind::ObjectArray => {
            let count = value.as_array().map_or(0, |items| items.len());
            if count == 0 {
                "-# *(not set)*".to_string()
            } else {
                format!(
                    "`{count} entr{}` - edit in dashboard",
                    if count == 1 { "y" } else { "ies" }
                )
            }
        }
        _ => {
            let text = value
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| value.to_string());
            if text.is_empty() {
                "-# *(not set)*".to_string()
            } else {
                format!("`{}`", cut(&text, 120))
            }
        }
    }
}

fn id_value(value: &Value, render: impl Fn(&str) -> String) -> String {
    value
        .as_str()
        .filter(|id| !id.is_empty())
        .map_or_else(|| "-# *(not set)*".to_string(), render)
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}
