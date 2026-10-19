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
use sea_orm::sea_query::OnConflict;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use serde_json::{Map, Value};

use lumi_db::entities::{global_module_state, guild_module_config, guild_module_state};

use crate::module::{ConfigField, ConfigFieldKind, ModuleRegistry};
use crate::{LumiError, Result};

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

/// Resolve the effective enabled flag: a global disable wins, then the guild
/// override, else the module is enabled.
fn resolve_enabled(global: Option<bool>, guild: Option<bool>) -> bool {
    global != Some(false) && guild.unwrap_or(true)
}

async fn global_enabled(db: &DatabaseConnection, module: &str) -> Option<bool> {
    global_module_state::Entity::find()
        .filter(global_module_state::Column::ModuleName.eq(module))
        .one(db)
        .await
        .ok()
        .flatten()
        .map(|row| row.enabled)
}

/// Whether a module runs in a guild.
pub async fn is_enabled(db: &DatabaseConnection, guild_id: u64, module: &str) -> bool {
    let guild = guild_module_state::Entity::find()
        .filter(guild_module_state::Column::GuildId.eq(guild_id.to_string()))
        .filter(guild_module_state::Column::ModuleName.eq(module))
        .one(db)
        .await
        .ok()
        .flatten()
        .map(|row| row.enabled);
    resolve_enabled(global_enabled(db, module).await, guild)
}

/// Persist a guild's enable/disable choice for a module.
pub async fn set_enabled(
    db: &DatabaseConnection,
    guild_id: u64,
    module: &str,
    enabled: bool,
) -> Result<()> {
    guild_module_state::Entity::insert(guild_module_state::ActiveModel {
        guild_id: Set(guild_id.to_string()),
        module_name: Set(module.to_string()),
        enabled: Set(enabled),
        updated_at: Set(ChronoUtc::now().fixed_offset()),
    })
    .on_conflict(
        OnConflict::columns([
            guild_module_state::Column::GuildId,
            guild_module_state::Column::ModuleName,
        ])
        .update_columns([
            guild_module_state::Column::Enabled,
            guild_module_state::Column::UpdatedAt,
        ])
        .to_owned(),
    )
    .exec(db)
    .await?;
    Ok(())
}

/// Every registered module with its resolved enabled state, sorted by name.
pub async fn list_feature_rows(
    registry: &ModuleRegistry,
    db: &DatabaseConnection,
    guild_id: u64,
) -> Vec<FeatureRow> {
    let guild_rows = load_states(
        guild_module_state::Entity::find()
            .filter(guild_module_state::Column::GuildId.eq(guild_id.to_string()))
            .all(db)
            .await,
        "guild module state",
        |row| (row.module_name, row.enabled),
    );
    let global_rows = load_states(
        global_module_state::Entity::find().all(db).await,
        "global module state",
        |row| (row.module_name, row.enabled),
    );

    let mut rows: Vec<FeatureRow> = registry
        .iter()
        .map(|module| {
            let meta = module.metadata();
            let name = module.name();
            FeatureRow {
                name: name.to_string(),
                display_name: meta.display_name,
                emoji: meta.emoji,
                description: meta.description,
                version: meta.version,
                enabled: resolve_enabled(global_rows.get(name).copied(), guild_rows.get(name).copied()),
            }
        })
        .collect();
    rows.sort_by(|a, b| a.display_name.to_lowercase().cmp(&b.display_name.to_lowercase()));
    rows
}

fn load_states<R>(
    result: std::result::Result<Vec<R>, sea_orm::DbErr>,
    label: &str,
    extract: impl Fn(R) -> (String, bool),
) -> HashMap<String, bool> {
    match result {
        Ok(rows) => rows.into_iter().map(extract).collect(),
        Err(err) => {
            tracing::warn!(error = %err, table = label, "failed to load module state; treating as unset");
            HashMap::new()
        }
    }
}

/// Stored per-guild overrides for a module, keyed by config key. Keys the guild
/// never set are absent; callers fall back to each field's `default`.
pub async fn get_config(
    db: &DatabaseConnection,
    guild_id: u64,
    module: &str,
) -> Map<String, Value> {
    match guild_module_config::Entity::find()
        .filter(guild_module_config::Column::GuildId.eq(guild_id.to_string()))
        .filter(guild_module_config::Column::ModuleName.eq(module))
        .all(db)
        .await
    {
        Ok(rows) => rows.into_iter().map(|row| (row.config_key, row.value)).collect(),
        Err(err) => {
            tracing::warn!(error = %err, module, "failed to load module config; treating as unset");
            Map::new()
        }
    }
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
    guild_module_config::Entity::insert(guild_module_config::ActiveModel {
        guild_id: Set(guild_id.to_string()),
        module_name: Set(module.to_string()),
        config_key: Set(key.to_string()),
        value: Set(value),
        updated_at: Set(ChronoUtc::now().fixed_offset()),
    })
    .on_conflict(
        OnConflict::columns([
            guild_module_config::Column::GuildId,
            guild_module_config::Column::ModuleName,
            guild_module_config::Column::ConfigKey,
        ])
        .update_columns([
            guild_module_config::Column::Value,
            guild_module_config::Column::UpdatedAt,
        ])
        .to_owned(),
    )
    .exec(db)
    .await?;
    Ok(())
}

/// Convert raw dashboard input into the field's JSON type.
fn coerce_value(raw: &str, kind: &ConfigFieldKind) -> Result<Value> {
    match kind {
        ConfigFieldKind::Boolean => Ok(Value::Bool(matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "true" | "1" | "yes"
        ))),
        ConfigFieldKind::Integer => raw
            .trim()
            .parse::<i64>()
            .map(Value::from)
            .map_err(|_| LumiError::Invalid(format!("'{raw}' is not an integer"))),
        _ => Ok(Value::String(raw.to_string())),
    }
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
    let value = coerce_value(raw, kind)?;
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
        ConfigFieldKind::RoleArray => {
            let ids: Vec<String> = value
                .as_array()
                .map(|array| array.iter().filter_map(parse_id).map(|id| format!("<@&{id}>")).collect())
                .unwrap_or_default();
            if ids.is_empty() {
                "-# *(not set)*".to_string()
            } else {
                cut(&ids.join(" "), 120)
            }
        }
        ConfigFieldKind::User => id_value(value, |id| format!("<@{id}>")),
        ConfigFieldKind::StringArray => {
            let items: Vec<String> = value
                .as_array()
                .map(|array| array.iter().filter_map(|item| item.as_str().map(str::to_string)).collect())
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
    parse_id(value).map_or_else(
        || "-# *(not set)*".to_string(),
        |id| render(&id.to_string()),
    )
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('\u{2026}');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn resolve_enabled_precedence() {
        assert!(!resolve_enabled(Some(false), Some(true)), "global disable wins");
        assert!(!resolve_enabled(None, Some(false)), "guild disable");
        assert!(resolve_enabled(None, None), "default enabled");
        assert!(resolve_enabled(Some(true), None));
    }

    #[test]
    fn parse_id_accepts_number_and_string() {
        assert_eq!(parse_id(&json!(42)), Some(42));
        assert_eq!(parse_id(&json!("42")), Some(42));
        assert_eq!(parse_id(&json!("nope")), None);
    }

    #[test]
    fn coerce_value_types() {
        assert_eq!(coerce_value("yes", &ConfigFieldKind::Boolean).ok(), Some(Value::Bool(true)));
        assert_eq!(coerce_value("nope", &ConfigFieldKind::Boolean).ok(), Some(Value::Bool(false)));
        assert_eq!(coerce_value(" 7 ", &ConfigFieldKind::Integer).ok(), Some(json!(7)));
        assert!(coerce_value("7.5", &ConfigFieldKind::Integer).is_err());
        assert_eq!(
            coerce_value("hi", &ConfigFieldKind::String).ok(),
            Some(Value::String("hi".into()))
        );
    }

    #[test]
    fn numeric_id_renders_as_mention() {
        let field = ConfigField {
            key: "channel".into(),
            label: "Channel".into(),
            description: String::new(),
            kind: ConfigFieldKind::Channel,
            default: Value::Null,
            subfields: Vec::new(),
            group: None,
        };
        assert_eq!(format_field_value(&field, Some(&json!(123))), "<#123>");
        assert_eq!(format_field_value(&field, None), "-# *(not set)*");
    }
}
