//! Process-local automod counters: per-member heat and per-guild mention
//! windows. A guild is owned by one shard process, so these stay in memory.
//!
//! Heat resets on restart, which is acceptable because it is a per-guild
//! anti-spam heuristic rather than durable moderation state.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

/// Entries above this count trigger an opportunistic prune of idle entries.
const PRUNE_AT: usize = 50_000;
/// Redis kept non-decaying heat for one hour; mirror that as the idle bound.
const IDLE_SECS: f64 = 3600.0;

struct HeatEntry {
    heat: f64,
    updated: Instant,
}

struct MentionWindow {
    count: u64,
    started: Instant,
}

#[derive(Default)]
struct State {
    heat: Mutex<HashMap<(u64, u64), HeatEntry>>,
    mentions: Mutex<HashMap<u64, MentionWindow>>,
}

fn state() -> &'static State {
    static STATE: OnceLock<State> = OnceLock::new();
    STATE.get_or_init(State::default)
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Heat with no updates for an hour is stale regardless of the guild's decay
/// setting; pruning must not depend on whichever guild triggered it.
fn idle(entry: &HeatEntry, now: Instant) -> bool {
    now.saturating_duration_since(entry.updated).as_secs_f64() >= IDLE_SECS
}

/// Add heat for a member, applying the lazy decay since the last update.
pub fn add_heat(guild_id: u64, user_id: u64, points: f64, decay_per_minute: f64) -> f64 {
    let now = Instant::now();
    let mut heat = lock(&state().heat);
    if heat.len() > PRUNE_AT {
        heat.retain(|_, entry| !idle(entry, now));
    }
    let entry = heat.entry((guild_id, user_id)).or_insert(HeatEntry {
        heat: 0.0,
        updated: now,
    });
    let decayed = if decay_per_minute > 0.0 {
        let minutes = now.saturating_duration_since(entry.updated).as_secs_f64() / 60.0;
        (entry.heat - minutes * decay_per_minute).max(0.0)
    } else {
        entry.heat.max(0.0)
    };
    entry.heat = decayed + points;
    entry.updated = now;
    entry.heat
}

/// Forget a member's accumulated heat (used after quarantine).
pub fn reset_heat(guild_id: u64, user_id: u64) {
    lock(&state().heat).remove(&(guild_id, user_id));
}

/// Add mentions to the guild's current window and return the window total.
pub fn record_mentions(guild_id: u64, count: u64, window_seconds: u64) -> u64 {
    let window = Duration::from_secs(window_seconds.max(1));
    let now = Instant::now();
    let mut mentions = lock(&state().mentions);
    if mentions.len() > PRUNE_AT {
        mentions.retain(|_, entry| now.saturating_duration_since(entry.started) < window);
    }
    let entry = mentions.entry(guild_id).or_insert(MentionWindow {
        count: 0,
        started: now,
    });
    if now.saturating_duration_since(entry.started) >= window {
        entry.count = 0;
        entry.started = now;
    }
    entry.count = entry.count.saturating_add(count);
    entry.count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heat_accumulates_and_resets() {
        reset_heat(1, 2);
        assert_eq!(add_heat(1, 2, 5.0, 0.0), 5.0);
        assert_eq!(add_heat(1, 2, 2.5, 0.0), 7.5);
        reset_heat(1, 2);
        assert_eq!(add_heat(1, 2, 1.0, 0.0), 1.0);
        reset_heat(1, 2);
    }

    #[test]
    fn mentions_accumulate_in_window() {
        assert_eq!(record_mentions(99, 2, 60), 2);
        assert_eq!(record_mentions(99, 3, 60), 5);
    }

    #[test]
    fn idle_heat_expires_after_the_cap() {
        let now = Instant::now();
        let fresh = HeatEntry {
            heat: 100.0,
            updated: now,
        };
        let stale = HeatEntry {
            heat: 1.0,
            updated: now - Duration::from_secs_f64(IDLE_SECS + 1.0),
        };
        assert!(!idle(&fresh, now));
        assert!(idle(&stale, now));
    }
}
