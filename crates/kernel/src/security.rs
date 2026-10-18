use std::collections::VecDeque;
use std::time::{Duration, Instant};

use chrono::Utc;
use dashmap::DashMap;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use serde::{Deserialize, Serialize};

use lumi_db::entities::enums::CaseAction;
use lumi_db::entities::panic_state;

use crate::error::Result;

pub const DEFAULT_WINDOW_SECS: u64 = 60;
pub const DEFAULT_COOLDOWN_SECS: u64 = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestructiveActionKind {
    Ban,
    Kick,
    ChannelDelete,
    RoleDelete,
    WebhookCreate,
    VanityChange,
    DangerousPermissionGrant,
    QuarantineBypass,
}

impl DestructiveActionKind {
    pub const fn default_limit(self) -> u32 {
        match self {
            Self::Ban | Self::Kick => 5,
            Self::ChannelDelete | Self::RoleDelete | Self::WebhookCreate => 3,
            Self::VanityChange | Self::DangerousPermissionGrant | Self::QuarantineBypass => 1,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ban => "ban",
            Self::Kick => "kick",
            Self::ChannelDelete => "channel_delete",
            Self::RoleDelete => "role_delete",
            Self::WebhookCreate => "webhook_create",
            Self::VanityChange => "vanity_change",
            Self::DangerousPermissionGrant => "dangerous_permission_grant",
            Self::QuarantineBypass => "quarantine_bypass",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AntiNukeResponse {
    Log,
    Quarantine,
    Ban,
}

impl Default for AntiNukeResponse {
    fn default() -> Self {
        Self::Quarantine
    }
}

#[derive(Clone, Debug)]
pub struct AntiNukeConfig {
    pub enabled: bool,
    pub window_secs: u64,
    pub limits: std::collections::HashMap<DestructiveActionKind, u32>,
    pub responses: std::collections::HashMap<DestructiveActionKind, AntiNukeResponse>,
}

impl Default for AntiNukeConfig {
    fn default() -> Self {
        let mut limits = std::collections::HashMap::new();
        let mut responses = std::collections::HashMap::new();
        for kind in [
            DestructiveActionKind::Ban,
            DestructiveActionKind::Kick,
            DestructiveActionKind::ChannelDelete,
            DestructiveActionKind::RoleDelete,
            DestructiveActionKind::WebhookCreate,
            DestructiveActionKind::VanityChange,
            DestructiveActionKind::DangerousPermissionGrant,
            DestructiveActionKind::QuarantineBypass,
        ] {
            limits.insert(kind, kind.default_limit());
            responses.insert(kind, AntiNukeResponse::Quarantine);
        }

        Self {
            enabled: true,
            window_secs: DEFAULT_WINDOW_SECS,
            limits,
            responses,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PanicStatus {
    pub active: bool,
    pub actor_id: Option<String>,
    pub invites_paused: bool,
    pub locked_channels: Vec<String>,
    pub started_at: Option<chrono::DateTime<chrono::FixedOffset>>,
}

#[derive(Clone, Debug)]
pub struct ActionThresholdViolation {
    pub kind: DestructiveActionKind,
    pub count: u32,
    pub limit: u32,
    pub response: AntiNukeResponse,
}

#[derive(Debug)]
pub struct SecurityManager {
    db: DatabaseConnection,
    panic_cache: DashMap<String, PanicStatus>,
    rate_trackers: DashMap<(String, String, DestructiveActionKind), VecDeque<Instant>>,
    tripped_cooldowns: DashMap<(String, String, DestructiveActionKind), Instant>,
}

impl SecurityManager {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            panic_cache: DashMap::new(),
            rate_trackers: DashMap::new(),
            tripped_cooldowns: DashMap::new(),
        }
    }

    pub async fn get_panic_status(&self, guild_id: &str) -> Result<Option<PanicStatus>> {
        if let Some(status) = self.panic_cache.get(guild_id) {
            if status.active {
                return Ok(Some(status.clone()));
            }
            return Ok(None);
        }

        let model = panic_state::Entity::find_by_id(guild_id.to_string())
            .one(&self.db)
            .await?;

        if let Some(state) = model {
            let locked_channels: Vec<String> = match &state.locked_channels {
                sea_orm::prelude::Json::Array(arr) => arr
                    .iter()
                    .filter_map(|v| v.as_str().map(ToString::to_string))
                    .collect(),
                sea_orm::prelude::Json::Object(map) => map.keys().cloned().collect(),
                _ => Vec::new(),
            };

            let status = PanicStatus {
                active: true,
                actor_id: Some(state.actor_id),
                invites_paused: state.invites_paused,
                locked_channels,
                started_at: Some(state.started_at),
            };

            self.panic_cache.insert(guild_id.to_string(), status.clone());
            Ok(Some(status))
        } else {
            Ok(None)
        }
    }

    pub async fn activate_panic(
        &self,
        guild_id: &str,
        actor_id: &str,
        invites_paused: bool,
        locked_channels: Vec<String>,
    ) -> Result<PanicStatus> {
        let now = Utc::now().fixed_offset();
        let channels_json = serde_json::to_value(&locked_channels)?;

        let active_model = panic_state::ActiveModel {
            guild_id: Set(guild_id.to_string()),
            actor_id: Set(actor_id.to_string()),
            invites_paused: Set(invites_paused),
            locked_channels: Set(channels_json),
            started_at: Set(now),
        };

        panic_state::Entity::insert(active_model)
            .on_conflict(
                sea_orm::sea_query::OnConflict::column(panic_state::Column::GuildId)
                    .update_columns([
                        panic_state::Column::ActorId,
                        panic_state::Column::InvitesPaused,
                        panic_state::Column::LockedChannels,
                        panic_state::Column::StartedAt,
                    ])
                    .to_owned(),
            )
            .exec(&self.db)
            .await?;

        let status = PanicStatus {
            active: true,
            actor_id: Some(actor_id.to_string()),
            invites_paused,
            locked_channels,
            started_at: Some(now),
        };

        self.panic_cache.insert(guild_id.to_string(), status.clone());
        Ok(status)
    }

    pub async fn deactivate_panic(&self, guild_id: &str) -> Result<Option<PanicStatus>> {
        let existing = self.get_panic_status(guild_id).await?;
        if existing.is_none() {
            return Ok(None);
        }

        panic_state::Entity::delete_by_id(guild_id.to_string())
            .exec(&self.db)
            .await?;

        self.panic_cache.remove(guild_id);
        Ok(existing)
    }

    pub fn record_destructive_action(
        &self,
        guild_id: &str,
        actor_id: &str,
        kind: DestructiveActionKind,
        config: &AntiNukeConfig,
    ) -> Option<ActionThresholdViolation> {
        if !config.enabled {
            return None;
        }

        let now = Instant::now();
        let window = Duration::from_secs(config.window_secs);
        let key = (guild_id.to_string(), actor_id.to_string(), kind);

        if let Some(tripped_at) = self.tripped_cooldowns.get(&key) {
            if now.duration_since(*tripped_at) < Duration::from_secs(DEFAULT_COOLDOWN_SECS) {
                return None;
            }
        }

        let mut queue = self.rate_trackers.entry(key.clone()).or_default();
        queue.push_back(now);

        while let Some(front) = queue.front() {
            if now.duration_since(*front) > window {
                queue.pop_front();
            } else {
                break;
            }
        }

        let count = queue.len() as u32;
        let limit = config
            .limits
            .get(&kind)
            .copied()
            .unwrap_or_else(|| kind.default_limit());

        if count > limit {
            self.tripped_cooldowns.insert(key, now);
            let response = config
                .responses
                .get(&kind)
                .copied()
                .unwrap_or(AntiNukeResponse::Quarantine);

            Some(ActionThresholdViolation {
                kind,
                count,
                limit,
                response,
            })
        } else {
            None
        }
    }

    pub async fn apply_quarantine(
        &self,
        guild_id: &str,
        target_user_id: &str,
        moderator_id: &str,
        reason: &str,
    ) -> Result<lumi_db::entities::moderation_case::Model> {
        let guild_id_u64: u64 = guild_id.parse().unwrap_or(0);
        let case_num = lumi_db::next_counter(&self.db, guild_id_u64 as i64, "cases").await?;

        let row = lumi_db::entities::moderation_case::ActiveModel {
            guild_id: Set(guild_id.to_string()),
            case_number: Set(case_num as i32),
            user_id: Set(target_user_id.to_string()),
            moderator_id: Set(moderator_id.to_string()),
            action: Set(CaseAction::Quarantine),
            custom_action: Set(None),
            reason: Set(Some(reason.to_string())),
            duration: Set(None),
            expires_at: Set(None),
            active: Set(true),
            created_at: Set(Utc::now().fixed_offset()),
            ..Default::default()
        };

        let model = row.insert(&self.db).await?;
        Ok(model)
    }

    pub fn get_active_count(&self, guild_id: &str, actor_id: &str, kind: DestructiveActionKind, window: Duration) -> u32 {
        let key = (guild_id.to_string(), actor_id.to_string(), kind);
        if let Some(queue) = self.rate_trackers.get(&key) {
            let now = Instant::now();
            queue
                .iter()
                .filter(|&&t| now.duration_since(t) <= window)
                .count() as u32
        } else {
            0
        }
    }
}
