//! Anti-nuke rate tracking and guild panic state.
//!
//! Action counts are held in-process per (guild, actor, action) and pruned
//! lazily; panic state is cached with a short TTL so external changes are
//! picked up without hammering the database.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use chrono::Utc;
use dashmap::DashMap;
use moka::future::Cache as MokaCache;
use sea_orm::{ActiveModelTrait, DatabaseConnection, EntityTrait, Set};
use serde::{Deserialize, Serialize};

use lumi_db::entities::enums::CaseAction;
use lumi_db::entities::panic_state;

use crate::{LumiError, Result};

pub const DEFAULT_WINDOW_SECS: u64 = 60;
pub const DEFAULT_COOLDOWN_SECS: u64 = 300;

/// How long a cached panic status may be stale before it is re-read.
const PANIC_CACHE_TTL: Duration = Duration::from_secs(60);

/// Prune stale action windows once every this many recorded actions.
const PRUNE_EVERY: u64 = 1024;

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

    /// Every kind, for building default config maps.
    pub const ALL: [Self; 8] = [
        Self::Ban,
        Self::Kick,
        Self::ChannelDelete,
        Self::RoleDelete,
        Self::WebhookCreate,
        Self::VanityChange,
        Self::DangerousPermissionGrant,
        Self::QuarantineBypass,
    ];

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AntiNukeResponse {
    Log,
    #[default]
    Quarantine,
    Ban,
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
        for kind in DestructiveActionKind::ALL {
            limits.insert(kind, kind.default_limit());
            responses.insert(kind, AntiNukeResponse::Quarantine);
        }

        Self {
            enabled: false,
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

type ActionKey = (String, String, DestructiveActionKind);

#[derive(Debug, Default)]
struct ActionWindow {
    events: VecDeque<Instant>,
    tripped_at: Option<Instant>,
    window: Duration,
}

/// Sliding-window counter for destructive actions, with a post-trip cooldown.
#[derive(Debug, Default)]
struct ActionTracker {
    windows: DashMap<ActionKey, ActionWindow>,
    recorded: AtomicU64,
}

impl ActionTracker {
    /// Record one action; `Some((count, limit))` when the threshold was just
    /// crossed. Repeats during the cooldown return `None`.
    fn record(
        &self,
        guild_id: &str,
        actor_id: &str,
        kind: DestructiveActionKind,
        window: Duration,
        limit: u32,
    ) -> Option<(u32, u32)> {
        let now = Instant::now();
        self.maybe_prune(now);

        let key = (guild_id.to_string(), actor_id.to_string(), kind);
        let mut entry = self.windows.entry(key).or_default();
        entry.window = window;

        let cooldown = Duration::from_secs(DEFAULT_COOLDOWN_SECS);
        if entry.tripped_at.is_some_and(|t| now.duration_since(t) < cooldown) {
            return None;
        }
        if entry.tripped_at.is_some() {
            entry.tripped_at = None;
            entry.events.clear();
        }

        while entry
            .events
            .front()
            .is_some_and(|t| now.duration_since(*t) > window)
        {
            entry.events.pop_front();
        }
        entry.events.push_back(now);

        // Only `limit + 1` events are ever needed to decide `count > limit`.
        let cap = usize::try_from(limit).unwrap_or(usize::MAX).saturating_add(1);
        while entry.events.len() > cap {
            entry.events.pop_front();
        }

        let count = u32::try_from(entry.events.len()).unwrap_or(u32::MAX);
        if count > limit {
            entry.tripped_at = Some(now);
            Some((count, limit))
        } else {
            None
        }
    }

    /// Drop windows that can no longer influence a decision.
    fn maybe_prune(&self, now: Instant) {
        if self.recorded.fetch_add(1, Ordering::Relaxed) % PRUNE_EVERY != 0 {
            return;
        }
        let cooldown = Duration::from_secs(DEFAULT_COOLDOWN_SECS);
        self.windows.retain(|_, window| {
            if window
                .tripped_at
                .is_some_and(|t| now.duration_since(t) < cooldown)
            {
                return true;
            }
            window
                .events
                .back()
                .is_some_and(|t| now.duration_since(*t) <= window.window)
        });
    }
}

#[derive(Debug)]
pub struct SecurityManager {
    db: DatabaseConnection,
    panic_cache: MokaCache<String, PanicStatus>,
    tracker: ActionTracker,
}

impl SecurityManager {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            db,
            panic_cache: MokaCache::builder().time_to_live(PANIC_CACHE_TTL).build(),
            tracker: ActionTracker::default(),
        }
    }

    pub async fn get_panic_status(&self, guild_id: &str) -> Result<Option<PanicStatus>> {
        if let Some(status) = self.panic_cache.get(guild_id).await {
            return Ok(Some(status));
        }

        let guild_snowflake: lumi_db::ids::GuildId = guild_id
            .parse()
            .map_err(|_| LumiError::Invalid(format!("guild id '{guild_id}' is not numeric")))?;

        let Some(state) = panic_state::Entity::find_by_id(guild_snowflake)
            .one(&self.db)
            .await?
        else {
            return Ok(None);
        };

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
            actor_id: Some(state.actor_id.to_string()),
            invites_paused: state.invites_paused,
            locked_channels,
            started_at: Some(state.started_at),
        };

        self.panic_cache.insert(guild_id.to_string(), status.clone()).await;
        Ok(Some(status))
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

        let guild_snowflake: lumi_db::ids::GuildId = guild_id
            .parse()
            .map_err(|_| LumiError::Invalid(format!("guild id '{guild_id}' is not numeric")))?;
        let actor_snowflake: lumi_db::ids::UserId = actor_id
            .parse()
            .map_err(|_| LumiError::Invalid(format!("actor id '{actor_id}' is not numeric")))?;

        panic_state::Entity::insert(panic_state::ActiveModel {
            guild_id: Set(guild_snowflake),
            actor_id: Set(actor_snowflake),
            invites_paused: Set(invites_paused),
            locked_channels: Set(channels_json),
            started_at: Set(now),
        })
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

        self.panic_cache.insert(guild_id.to_string(), status.clone()).await;
        Ok(status)
    }

    pub async fn deactivate_panic(&self, guild_id: &str) -> Result<Option<PanicStatus>> {
        let existing = self.get_panic_status(guild_id).await?;
        if existing.is_none() {
            return Ok(None);
        }

        let guild_snowflake: lumi_db::ids::GuildId = guild_id
            .parse()
            .map_err(|_| LumiError::Invalid(format!("guild id '{guild_id}' is not numeric")))?;

        panic_state::Entity::delete_by_id(guild_snowflake)
            .exec(&self.db)
            .await?;

        self.panic_cache.invalidate(guild_id).await;
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

        let window = Duration::from_secs(config.window_secs);
        let limit = config
            .limits
            .get(&kind)
            .copied()
            .unwrap_or_else(|| kind.default_limit());
        let (count, limit) = self.tracker.record(guild_id, actor_id, kind, window, limit)?;
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
    }

    pub async fn apply_quarantine(
        &self,
        guild_id: &str,
        target_user_id: &str,
        moderator_id: &str,
        reason: &str,
    ) -> Result<lumi_db::entities::moderation_case::Model> {
        let guild_id_u64: u64 = guild_id
            .parse()
            .map_err(|_| LumiError::Invalid(format!("guild id '{guild_id}' is not numeric")))?;
        let case_num = lumi_db::next_counter(&self.db, guild_id_u64, "cases").await?;
        let case_number = i32::try_from(case_num)
            .map_err(|_| LumiError::Other(format!("case counter overflow for guild {guild_id}")))?;

        let target_snowflake: lumi_db::ids::UserId = target_user_id
            .parse()
            .map_err(|_| LumiError::Invalid(format!("target user id '{target_user_id}' is not numeric")))?;
        let mod_snowflake: lumi_db::ids::UserId = moderator_id
            .parse()
            .map_err(|_| LumiError::Invalid(format!("moderator id '{moderator_id}' is not numeric")))?;

        let row = lumi_db::entities::moderation_case::ActiveModel {
            guild_id: Set(lumi_db::ids::GuildId::from(guild_id_u64)),
            case_number: Set(case_number),
            user_id: Set(target_snowflake),
            moderator_id: Set(mod_snowflake),
            action: Set(CaseAction::Quarantine),
            custom_action: Set(None),
            reason: Set(Some(reason.to_string())),
            duration: Set(None),
            expires_at: Set(None),
            active: Set(true),
            created_at: Set(Utc::now().fixed_offset()),
            ..Default::default()
        };

        Ok(row.insert(&self.db).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trips_after_limit_and_cools_down() {
        let tracker = ActionTracker::default();
        let window = Duration::from_secs(60);
        let mut violation = None;
        for _ in 0..=5 {
            violation = tracker.record("1", "2", DestructiveActionKind::Ban, window, 5);
        }
        assert_eq!(violation, Some((6, 5)));

        // Immediately after tripping the cooldown suppresses further reports.
        assert_eq!(
            tracker.record("1", "2", DestructiveActionKind::Ban, window, 5),
            None
        );
    }

    #[test]
    fn under_limit_does_not_trip() {
        let tracker = ActionTracker::default();
        let window = Duration::from_secs(60);
        for _ in 0..5 {
            assert_eq!(tracker.record("1", "3", DestructiveActionKind::Kick, window, 5), None);
        }
    }

    #[test]
    fn window_bounds_memory_to_limit_plus_one() {
        let tracker = ActionTracker::default();
        let window = Duration::from_secs(3600);
        for _ in 0..50 {
            let _ = tracker.record("1", "9", DestructiveActionKind::RoleDelete, window, 3);
        }
        let key = ("1".to_string(), "9".to_string(), DestructiveActionKind::RoleDelete);
        let len = tracker.windows.get(&key).map_or(0, |w| w.events.len());
        assert!(len <= 4, "window kept {len} events");
    }
}
