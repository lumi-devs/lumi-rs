//! Shared module-config plumbing.
//!
//! Modules read their values from the single `guild_module_config` table via
//! [`lumi_kernel::module_state`]. These helpers keep the JSON coercion rules
//! (numeric strings, role/channel snowflakes, boolean fallbacks) and the
//! `ConfigField` boilerplate in one place instead of each module re-inventing
//! them. A field a guild never set is absent from the map; callers pass the
//! schema default as the fallback.

use serde_json::{Map, Value, json};

use lumi_kernel::module::{ConfigField, ConfigFieldKind};
use lumi_kernel::module_state;

/// Build one config field with the shared empty `subfields` representation.
#[must_use]
pub fn field(
    key: &str,
    label: &str,
    description: &str,
    kind: ConfigFieldKind,
    default: Value,
    group: &str,
) -> ConfigField {
    ConfigField {
        key: key.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        kind,
        default,
        subfields: Vec::new(),
        group: Some(group.to_string()),
    }
}

#[must_use]
pub fn bool_field(
    key: &str,
    label: &str,
    description: &str,
    default: bool,
    group: &str,
) -> ConfigField {
    field(
        key,
        label,
        description,
        ConfigFieldKind::Boolean,
        json!(default),
        group,
    )
}

#[must_use]
pub fn int_field(
    key: &str,
    label: &str,
    description: &str,
    default: i64,
    group: &str,
) -> ConfigField {
    field(
        key,
        label,
        description,
        ConfigFieldKind::Integer,
        json!(default),
        group,
    )
}

#[must_use]
pub fn string_field(
    key: &str,
    label: &str,
    description: &str,
    default: &str,
    group: &str,
) -> ConfigField {
    field(
        key,
        label,
        description,
        ConfigFieldKind::String,
        json!(default),
        group,
    )
}

#[must_use]
pub fn string_array_field(key: &str, label: &str, description: &str, group: &str) -> ConfigField {
    field(
        key,
        label,
        description,
        ConfigFieldKind::StringArray,
        json!([]),
        group,
    )
}

#[must_use]
pub fn channel_field(key: &str, label: &str, description: &str, group: &str) -> ConfigField {
    field(
        key,
        label,
        description,
        ConfigFieldKind::Channel,
        Value::Null,
        group,
    )
}

#[must_use]
pub fn role_field(key: &str, label: &str, description: &str, group: &str) -> ConfigField {
    field(
        key,
        label,
        description,
        ConfigFieldKind::Role,
        Value::Null,
        group,
    )
}

#[must_use]
pub fn role_array_field(key: &str, label: &str, description: &str, group: &str) -> ConfigField {
    field(
        key,
        label,
        description,
        ConfigFieldKind::RoleArray,
        json!([]),
        group,
    )
}

#[must_use]
pub fn enum_field(
    key: &str,
    label: &str,
    description: &str,
    options: &[&str],
    default: &str,
    group: &str,
) -> ConfigField {
    field(
        key,
        label,
        description,
        ConfigFieldKind::Enum(options.iter().map(ToString::to_string).collect()),
        json!(default),
        group,
    )
}

/// A key present in the guild config, or `None`.
#[must_use]
pub fn value<'a>(map: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    map.get(key)
}

/// Snowflake for `key`, accepting a JSON number or a numeric string.
#[must_use]
pub fn id(map: &Map<String, Value>, key: &str) -> Option<u64> {
    value(map, key).and_then(module_state::parse_id)
}

/// Array of snowflakes; unparsable entries are skipped.
#[must_use]
pub fn id_list(map: &Map<String, Value>, key: &str) -> Vec<u64> {
    value(map, key)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(module_state::parse_id).collect())
        .unwrap_or_default()
}

/// Array of strings; non-string entries are skipped.
#[must_use]
pub fn string_list(map: &Map<String, Value>, key: &str) -> Vec<String> {
    value(map, key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Boolean value, falling back when unset or of another type.
#[must_use]
pub fn boolean(map: &Map<String, Value>, key: &str, fallback: bool) -> bool {
    value(map, key).and_then(Value::as_bool).unwrap_or(fallback)
}

/// Signed integer, accepting a JSON number or a numeric string.
#[must_use]
pub fn integer(map: &Map<String, Value>, key: &str, fallback: i64) -> i64 {
    value(map, key)
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .unwrap_or(fallback)
}

/// Non-negative integer, clamped at zero.
#[must_use]
pub fn unsigned(map: &Map<String, Value>, key: &str, fallback: u64) -> u64 {
    value(map, key)
        .and_then(module_state::parse_id)
        .unwrap_or(fallback)
}
