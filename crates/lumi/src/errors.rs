//! Deduplicated framework error log.
//!
//! Poise calls [`on_error`] for every user-code error and command panic. Errors
//! are collapsed by command plus traceback so a failure loop cannot flood the
//! log, and each key keeps its occurrence count and first/last sighting.

use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use lumi_kernel::bot::{Data, Error};

/// One deduplicated error key and its sightings.
#[derive(Debug, Clone)]
pub struct ErrorRecord {
    pub count: u64,
    pub first_seen: Instant,
    pub last_seen: Instant,
    pub traceback: String,
}

static ERRORS: LazyLock<Mutex<BTreeMap<String, ErrorRecord>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

/// Record `traceback` under `key`, returning the occurrence count and how long
/// ago this key was first seen (seconds).
fn record(key: String, traceback: String) -> (u64, u64) {
    let now = Instant::now();
    let mut errors = ERRORS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let entry = errors.entry(key).or_insert_with(|| ErrorRecord {
        count: 0,
        first_seen: now,
        last_seen: now,
        traceback: traceback.clone(),
    });
    entry.count += 1;
    entry.last_seen = now;
    entry.traceback = traceback;
    let result = (entry.count, entry.first_seen.elapsed().as_secs());
    drop(errors);
    result
}

/// Number of distinct error keys recorded since start.
#[must_use]
pub fn distinct_count() -> usize {
    ERRORS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .len()
}

/// Snapshot of every deduplicated error, newest first.
#[must_use]
pub fn snapshot() -> Vec<(String, ErrorRecord)> {
    let errors = ERRORS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut rows: Vec<_> = errors
        .iter()
        .map(|(key, rec)| (key.clone(), rec.clone()))
        .collect();
    drop(errors);
    rows.sort_by_key(|row| std::cmp::Reverse(row.1.last_seen));
    rows
}

/// Flatten an error into one traceback string.
///
/// `anyhow::Error` deliberately does not implement `std::error::Error`, so the
/// command variant is formatted with anyhow's alternate display (full chain)
/// instead of walking `source()`.
fn traceback(error: &poise::FrameworkError<'_, Data, Error>) -> String {
    match error {
        poise::FrameworkError::Command { error, .. } => format!("{error:#}"),
        poise::FrameworkError::CommandPanic { payload, .. } => payload.as_deref().map_or_else(
            || "command panicked".to_string(),
            |payload| format!("command panicked: {payload}"),
        ),
        other => other.to_string(),
    }
}

/// Dedupe key: command name when known, plus the full traceback.
fn dedupe_key(error: &poise::FrameworkError<'_, Data, Error>) -> String {
    let command = error
        .ctx()
        .map_or_else(String::new, |ctx| ctx.command().qualified_name.to_string());
    format!("{command}: {}", traceback(error))
}

/// Errors the user caused (bad arguments, cooldown, missing permission) are
/// not internal failures and must not page anyone.
fn is_user_error(error: &poise::FrameworkError<'_, Data, Error>) -> bool {
    use poise::FrameworkError as E;
    matches!(
        error,
        E::SubcommandRequired { .. }
            | E::ArgumentParse { .. }
            | E::CooldownHit { .. }
            | E::MissingUserPermissions { .. }
            | E::NotAnOwner { .. }
            | E::GuildOnly { .. }
            | E::DmOnly { .. }
            | E::NsfwOnly { .. }
            | E::CommandCheckFailed { .. }
            | E::UnknownCommand { .. }
            | E::UnknownInteraction { .. }
            | E::NonCommandMessage { .. }
    )
}

/// Poise `FrameworkOptions::on_error` hook: log one deduplicated line, then
/// defer to the default user-facing reply.
pub async fn on_error(error: poise::FrameworkError<'_, Data, Error>) {
    let traceback = traceback(&error);
    let (count, age_secs) = record(dedupe_key(&error), traceback.clone());
    let class = if is_user_error(&error) {
        "user"
    } else {
        "internal"
    };

    let command = error
        .ctx()
        .map_or_else(String::new, |ctx| ctx.command().qualified_name.to_string());
    let guild_id = error.ctx().and_then(|ctx| ctx.guild_id()).map(|id| id.get());
    let channel_id = error.ctx().map(|ctx| ctx.channel_id().get());
    let user_id = error.ctx().map(|ctx| ctx.author().id.get());

    metrics::counter!("lumi.command.failed", "command" => command.clone(), "class" => class)
        .increment(1);
    let span = tracing::error_span!(
        "command_error",
        command = %command,
        class,
        guild_id = ?guild_id,
        channel_id = ?channel_id,
        user_id = ?user_id,
    );
    let entered = span.enter();
    if class == "user" {
        tracing::warn!(
            occurrences = count,
            first_seen_secs_ago = age_secs,
            "user error: {traceback}"
        );
    } else {
        tracing::error!(
            occurrences = count,
            first_seen_secs_ago = age_secs,
            "command error: {traceback}"
        );
    }
    drop(entered);

    if let Err(err) = poise::builtins::on_error(error).await {
        tracing::error!(error = %err, "failed to deliver error response");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_errors_collapse_with_counts() {
        let before = distinct_count();
        let (count_a, _) = record("k: boom".to_string(), "boom".to_string());
        let (count_b, _) = record("k: boom".to_string(), "boom".to_string());
        assert_eq!(count_a, 1);
        assert_eq!(count_b, 2);
        assert_eq!(distinct_count(), before + 1);

        let key = "k: boom".to_string();
        let stored = ERRORS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&key)
            .cloned();
        assert_eq!(stored.map(|rec| rec.count), Some(2));
    }
}
