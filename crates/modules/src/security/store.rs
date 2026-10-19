use std::collections::HashMap;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use lumi_db::entities::enums::CaseAction;
use lumi_db::entities::{guild_config, moderation_case};
use lumi_kernel::security::{
    AntiNukeConfig, AntiNukeResponse, DestructiveActionKind, DEFAULT_WINDOW_SECS,
};
use lumi_kernel::{Result, Services};

const MODULE: &str = "security";
const MOD_CONFIG_MODULE: &str = "mod";
// shortcut: anti-nuke config is read from `guild_config` (same as the logging
// module); the hub Modules tab writes `guild_module_config`. Point one at the
// other when the security config UI lands.

/// Anti-nuke runtime settings: the threshold config plus the non-kernel extras
/// (trusted roles exempt from enforcement and where alerts are posted).
#[derive(Clone, Debug)]
pub struct AntiNukeSettings {
    pub config: AntiNukeConfig,
    pub trusted_roles: Vec<u64>,
    pub log_channel: Option<u64>,
}

/// Every stored config override for the security module, keyed by config key.
pub async fn module_config(
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<HashMap<String, serde_json::Value>> {
    Ok(guild_config::Entity::find()
        .filter(guild_config::Column::GuildId.eq(guild_id as i64))
        .filter(guild_config::Column::Module.eq(MODULE))
        .all(db)
        .await?
        .into_iter()
        .map(|model| (model.key, model.value))
        .collect())
}

fn id_value(value: &serde_json::Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
}

fn number(map: &HashMap<String, serde_json::Value>, key: &str, fallback: u64) -> u64 {
    map.get(key).and_then(id_value).unwrap_or(fallback)
}

fn boolean(map: &HashMap<String, serde_json::Value>, key: &str, fallback: bool) -> bool {
    map.get(key).and_then(serde_json::Value::as_bool).unwrap_or(fallback)
}

fn id_list(map: &HashMap<String, serde_json::Value>, key: &str) -> Vec<u64> {
    map.get(key)
        .and_then(serde_json::Value::as_array)
        .map(|items| items.iter().filter_map(id_value).collect())
        .unwrap_or_default()
}

fn response(map: &HashMap<String, serde_json::Value>, key: &str) -> AntiNukeResponse {
    match map.get(key).and_then(serde_json::Value::as_str) {
        Some("log") => AntiNukeResponse::Log,
        Some("ban") => AntiNukeResponse::Ban,
        _ => AntiNukeResponse::Quarantine,
    }
}

/// Build the anti-nuke config from guild config. `enabled` defaults to `false`
/// so an unconfigured guild is never enforced with empty settings.
pub async fn anti_nuke_settings(
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<AntiNukeSettings> {
    let map = module_config(db, guild_id).await?;

    let mut limits = HashMap::new();
    let mut responses = HashMap::new();
    for (kind, limit_key, response_key) in [
        (DestructiveActionKind::Ban, "max_bans", Some("response_bans")),
        (DestructiveActionKind::Kick, "max_kicks", Some("response_kicks")),
        (
            DestructiveActionKind::ChannelDelete,
            "max_channel_deletes",
            Some("response_channel_deletes"),
        ),
        (
            DestructiveActionKind::RoleDelete,
            "max_role_deletes",
            Some("response_role_deletes"),
        ),
        (
            DestructiveActionKind::WebhookCreate,
            "max_webhook_creates",
            Some("response_webhook_creates"),
        ),
        (DestructiveActionKind::VanityChange, "max_vanity_changes", None),
        (
            DestructiveActionKind::DangerousPermissionGrant,
            "max_permission_grants",
            None,
        ),
        (
            DestructiveActionKind::QuarantineBypass,
            "max_quarantine_bypass",
            None,
        ),
    ] {
        limits.insert(kind, number(&map, limit_key, u64::from(kind.default_limit())) as u32);
        responses.insert(
            kind,
            response_key.map_or(AntiNukeResponse::Quarantine, |key| response(&map, key)),
        );
    }

    Ok(AntiNukeSettings {
        config: AntiNukeConfig {
            enabled: boolean(&map, "antinuke_enabled", false),
            window_secs: number(&map, "window_seconds", DEFAULT_WINDOW_SECS),
            limits,
            responses,
        },
        trusted_roles: id_list(&map, "trusted_role_ids"),
        log_channel: map.get("log_channel_id").and_then(id_value),
    })
}

/// The configured quarantine role id (owned by the moderation module), or
/// `None` when quarantine is unconfigured.
pub async fn quarantine_role(
    db: &DatabaseConnection,
    guild_id: u64,
) -> Result<Option<u64>> {
    Ok(guild_config::Entity::find()
        .filter(guild_config::Column::GuildId.eq(guild_id as i64))
        .filter(guild_config::Column::Module.eq(MOD_CONFIG_MODULE))
        .filter(guild_config::Column::Key.eq("quarantine_role_id"))
        .one(db)
        .await?
        .and_then(|model| id_value(&model.value)))
}

/// Whether panic mode should lock permission-gated commands for this guild:
/// panic is active and `panic_lock_mod_commands` is enabled.
pub async fn panic_locks_commands(services: &Services, guild_id: u64) -> bool {
    // Check the in-memory panic cache first so the config read only happens
    // while a guild is actually in lockdown.
    if !services
        .security
        .get_panic_status(&guild_id.to_string())
        .await
        .is_ok_and(|status| status.is_some())
    {
        return false;
    }
    let Ok(map) = module_config(&services.db, guild_id).await else {
        return false;
    };
    boolean(&map, "panic_lock_mod_commands", true)
}

/// Record an anti-nuke alert as a moderation case.
pub async fn create_alert_case(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    moderator_id: u64,
    reason: &str,
) -> Result<moderation_case::Model> {
    let case_num = lumi_db::next_counter(db, guild_id as i64, "cases").await?;
    let row = moderation_case::ActiveModel {
        guild_id: Set(guild_id.to_string()),
        case_number: Set(case_num as i32),
        user_id: Set(user_id.to_string()),
        moderator_id: Set(moderator_id.to_string()),
        action: Set(CaseAction::AntinukeAlert),
        custom_action: Set(None),
        reason: Set(Some(reason.to_string())),
        duration: Set(None),
        expires_at: Set(None),
        active: Set(true),
        created_at: Set(Utc::now().fixed_offset()),
        ..Default::default()
    };
    Ok(row.insert(db).await?)
}
