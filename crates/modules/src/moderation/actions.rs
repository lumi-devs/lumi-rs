//! Shared punishment plumbing: applying Discord actions, recording their case,
//! and delivering the log/notification fan-out exactly once.

use std::future::Future;
use std::time::Duration;

use poise::serenity_prelude as serenity;
use serenity::all::{EditMember, GuildId, Http, Timestamp, UserId};

use lumi_db::entities::{enums::CaseAction, moderation_case};
use lumi_ui::Card;

use super::store::{self, CreateCaseParams, ModConfig};
use super::views::{action_color, action_emoji, action_label, case_card};

/// Discord caps communication timeouts at 28 days.
pub const MAX_TIMEOUT: Duration = Duration::from_secs(28 * 86_400);

/// One target a punishment was successfully applied to. `case_number` is
/// `None` when the Discord action worked but the case row could not be written.
#[derive(Clone, Debug)]
pub struct AppliedCase {
    pub target: UserId,
    /// `None` when Discord applied the action but the case row failed to write.
    pub case: Option<moderation_case::Model>,
}

/// Result of a bulk punishment, including partial failures.
#[derive(Clone, Debug)]
pub struct BulkOutcome {
    pub action: CaseAction,
    pub applied: Vec<AppliedCase>,
    pub failures: Vec<(UserId, String)>,
}

impl BulkOutcome {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.applied.is_empty()
    }
}

/// Record a case, logging instead of failing the Discord action that already
/// happened. A missing case never hides the punishment from the moderator.
pub(crate) async fn record_case(
    db: &sea_orm::DatabaseConnection,
    guild_id: u64,
    target: UserId,
    moderator_id: u64,
    action: CaseAction,
    reason: Option<&str>,
    duration_seconds: Option<i32>,
) -> Option<moderation_case::Model> {
    match store::create_case(
        db,
        CreateCaseParams {
            guild_id,
            user_id: target.get(),
            moderator_id,
            action,
            reason,
            duration_seconds,
        },
    )
    .await
    {
        Ok(case) => Some(case),
        Err(error) => {
            tracing::error!(
                %error,
                guild_id,
                user_id = target.get(),
                action = action_label(&action),
                "moderation action applied but case could not be recorded"
            );
            None
        }
    }
}

/// Apply `punish` to every target, recording one case per success and keeping
/// successful actions visible even when the database write fails.
pub async fn apply_bulk<F, Fut>(
    db: &sea_orm::DatabaseConnection,
    guild_id: u64,
    moderator_id: u64,
    action: CaseAction,
    reason: Option<&str>,
    targets: Vec<UserId>,
    punish: F,
) -> BulkOutcome
where
    F: Fn(UserId) -> Fut,
    Fut: Future<Output = serenity::Result<()>>,
{
    let mut outcome = BulkOutcome {
        action,
        applied: Vec::with_capacity(targets.len()),
        failures: Vec::new(),
    };

    for target in targets {
        match punish(target).await {
            Ok(()) => {
                let case =
                    record_case(db, guild_id, target, moderator_id, action, reason, None).await;
                outcome.applied.push(AppliedCase { target, case });
            }
            Err(error) => {
                tracing::warn!(%error, guild_id, user_id = target.get(), "moderation action failed");
                outcome.failures.push((target, error.to_string()));
            }
        }
    }

    outcome
}

/// Timeout a member in one API call, clamped to Discord's 28-day maximum.
pub async fn apply_timeout(
    http: &Http,
    guild_id: GuildId,
    target: UserId,
    duration: Duration,
    reason: &str,
) -> serenity::Result<Timestamp> {
    let seconds = i64::try_from(duration.min(MAX_TIMEOUT).as_secs()).unwrap_or(i64::MAX);
    let timestamp = Timestamp::now().unix_timestamp().saturating_add(seconds);
    let until = Timestamp::from_unix_timestamp(timestamp).map_err(|error| {
        serenity::Error::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, error))
    })?;

    guild_id
        .edit_member(
            http,
            target,
            EditMember::new()
                .disable_communication_until(until)
                .audit_log_reason(reason),
        )
        .await?;
    Ok(until)
}

/// Clear a member timeout in one API call.
pub async fn clear_timeout(
    http: &Http,
    guild_id: GuildId,
    target: UserId,
    reason: &str,
) -> serenity::Result<()> {
    guild_id
        .edit_member(
            http,
            target,
            EditMember::new()
                .enable_communication()
                .audit_log_reason(reason),
        )
        .await?;
    Ok(())
}

fn notifies_target(action: CaseAction) -> bool {
    matches!(
        action,
        CaseAction::Warn
            | CaseAction::Kick
            | CaseAction::Ban
            | CaseAction::Softban
            | CaseAction::Mute
            | CaseAction::Quarantine
    )
}

/// DM the target (when enabled and appropriate) and post the case to the
/// configured mod-log channel. Delivery problems are logged, never surfaced as
/// command failures: the punishment itself already succeeded.
pub(crate) async fn notify(
    http: &Http,
    config: &ModConfig,
    target: UserId,
    action: CaseAction,
    case: Option<&moderation_case::Model>,
    reason: Option<&str>,
) {
    if config.dm_on_action && notifies_target(action) {
        let mut lines = vec![format!("**Action:** {}", action_label(&action))];
        if let Some(reason) = reason {
            lines.push(format!("**Reason:** {reason}"));
        }
        if let Some(duration) = case.and_then(|case| case.duration) {
            lines.push(format!("**Duration:** {}", super::views::human_seconds(duration)));
        }
        let card = Card::lines(
            format!("{} Moderation Action", action_emoji(&action)),
            lines,
        )
        .accent(Some(action_color(&action)));
        if let Err(error) = target.dm(http, card.into_reply().message()).await {
            tracing::debug!(%error, user_id = target.get(), "could not DM moderated member");
        }
    }

    let Some(channel) = config.log_channel else {
        return;
    };
    let Some(case) = case else {
        return;
    };
    if let Err(error) = serenity::all::GenericChannelId::new(channel)
        .send_message(http, case_card(case).message())
        .await
    {
        tracing::warn!(%error, channel_id = channel, "could not post moderation case to log channel");
    }
}
