use std::collections::HashMap;

use sea_orm::DatabaseConnection;
use serde_json::{Map, Value};

use lumi_db::entities::{enums::CaseAction, moderation_case};
use lumi_kernel::module_state;
use lumi_kernel::security::{
    AntiNukeConfig, AntiNukeResponse, DEFAULT_WINDOW_SECS, DestructiveActionKind,
};
use lumi_kernel::{Result, Services};

use crate::config;

const MODULE: &str = "security";

/// Anti-nuke runtime settings: the threshold config plus the non-kernel extras
/// (trusted roles exempt from enforcement and where alerts are posted).
#[derive(Clone, Debug)]
pub struct AntiNukeSettings {
    pub config: AntiNukeConfig,
    pub trusted_roles: Vec<u64>,
    pub log_channel: Option<u64>,
}

fn response(map: &Map<String, Value>, key: &str) -> AntiNukeResponse {
    match config::value(map, key).and_then(Value::as_str) {
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
    let map = module_state::get_config(db, guild_id, MODULE).await;

    let mut limits = HashMap::new();
    let mut responses = HashMap::new();
    for (kind, limit_key, response_key) in [
        (
            DestructiveActionKind::Ban,
            "max_bans",
            Some("response_bans"),
        ),
        (
            DestructiveActionKind::Kick,
            "max_kicks",
            Some("response_kicks"),
        ),
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
        (
            DestructiveActionKind::VanityChange,
            "max_vanity_changes",
            None,
        ),
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
        let limit = config::unsigned(&map, limit_key, u64::from(kind.default_limit())).max(1);
        limits.insert(kind, u32::try_from(limit).unwrap_or(u32::MAX));
        responses.insert(
            kind,
            response_key.map_or(AntiNukeResponse::Quarantine, |key| response(&map, key)),
        );
    }

    Ok(AntiNukeSettings {
        config: AntiNukeConfig {
            enabled: config::boolean(&map, "antinuke_enabled", false),
            window_secs: config::unsigned(&map, "window_seconds", DEFAULT_WINDOW_SECS).max(1),
            limits,
            responses,
        },
        trusted_roles: config::id_list(&map, "trusted_role_ids"),
        log_channel: config::id(&map, "log_channel_id"),
    })
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
    let map = module_state::get_config(&services.db, guild_id, MODULE).await;
    config::boolean(&map, "panic_lock_mod_commands", true)
}

/// Record an anti-nuke alert as a moderation case.
///
/// Case rows are written through the moderation store so numbering, expiry and
/// column defaults stay in one place.
pub async fn create_alert_case(
    db: &DatabaseConnection,
    guild_id: u64,
    user_id: u64,
    moderator_id: u64,
    reason: &str,
) -> Result<moderation_case::Model> {
    crate::moderation::store::create_case(
        db,
        crate::moderation::store::CreateCaseParams {
            guild_id,
            user_id,
            moderator_id,
            action: CaseAction::AntinukeAlert,
            reason: Some(reason),
            duration_seconds: None,
        },
    )
    .await
}
