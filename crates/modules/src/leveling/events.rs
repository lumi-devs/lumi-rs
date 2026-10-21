use std::sync::OnceLock;
use std::time::{Duration, Instant};

use poise::serenity_prelude as serenity;
use serenity::all::{ChannelId, GuildId, UserId, VoiceState};

use lumi_kernel::Result;
use lumi_kernel::dashmap::DashMap;

use super::store;

const DEFAULT_MESSAGE_XP: i64 = 15;
const VOICE_XP_PER_MINUTE: i64 = 5;
/// A lost voice event can leave a session behind; prune old ones once the map
/// grows past this size rather than on every update.
const SESSION_PRUNE_AT: usize = 50_000;
/// No real voice session earns for longer than this; older entries are stale.
const SESSION_MAX_AGE: Duration = Duration::from_secs(6 * 60 * 60);

static VOICE_SESSIONS: OnceLock<DashMap<(u64, u64), Instant>> = OnceLock::new();

fn voice_sessions() -> &'static DashMap<(u64, u64), Instant> {
    VOICE_SESSIONS.get_or_init(DashMap::new)
}

fn stale_session(started: Instant, now: Instant) -> bool {
    now.saturating_duration_since(started) > SESSION_MAX_AGE
}

fn prune_sessions(sessions: &DashMap<(u64, u64), Instant>, now: Instant) {
    if sessions.len() < SESSION_PRUNE_AT {
        return;
    }
    sessions.retain(|_, started| !stale_session(*started, now));
}

fn is_bot(state: &VoiceState) -> bool {
    state.member.as_ref().is_some_and(|m| m.user.bot())
}

fn humans_in_channel(ctx: &serenity::Context, guild_id: GuildId, channel_id: ChannelId) -> usize {
    ctx.cache.guild(guild_id).map_or(0, |guild| {
        guild
            .voice_states
            .iter()
            .filter(|state| {
                state.channel_id == Some(channel_id)
                    && !state.member.as_ref().is_some_and(|m| m.user.bot())
            })
            .count()
    })
}

/// Audible = connected, not muted/deafened/suppressed, and not alone.
fn is_audible(ctx: &serenity::Context, state: &VoiceState) -> bool {
    let Some(channel_id) = state.channel_id else {
        return false;
    };
    if state.mute() || state.deaf() || state.self_mute() || state.self_deaf() || state.suppress() {
        return false;
    }
    if is_bot(state) {
        return false;
    }
    let Some(guild_id) = state.guild_id else {
        return false;
    };
    humans_in_channel(ctx, guild_id, channel_id) >= 2
}

fn start_session(guild_id: u64, user_id: u64) {
    let sessions = voice_sessions();
    prune_sessions(sessions, Instant::now());
    sessions
        .entry((guild_id, user_id))
        .or_insert_with(Instant::now);
}

/// Stop a session and bank the whole minutes earned. No-op when not started.
async fn flush_session(
    services: &lumi_kernel::Services,
    guild_id: u64,
    user_id: u64,
) -> Result<()> {
    let Some((_, started)) = voice_sessions().remove(&(guild_id, user_id)) else {
        return Ok(());
    };
    let minutes = i32::try_from(started.elapsed().as_secs() / 60).unwrap_or(i32::MAX);
    if minutes < 1 {
        return Ok(());
    }
    store::add_voice_minutes(
        &services.db,
        guild_id,
        user_id,
        minutes,
        i64::from(minutes).saturating_mul(VOICE_XP_PER_MINUTE),
    )
    .await?;
    Ok(())
}

/// Reconcile every human in `channel_id` after an occupancy change, so the
/// members who were already sitting there start/stop earning too.
async fn sync_channel(
    ctx: &serenity::Context,
    services: &lumi_kernel::Services,
    guild_id: GuildId,
    channel_id: ChannelId,
    skip: Option<UserId>,
) -> Result<()> {
    let states: Vec<(UserId, bool)> = {
        let Some(guild) = ctx.cache.guild(guild_id) else {
            return Ok(());
        };
        guild
            .voice_states
            .iter()
            .filter(|state| state.channel_id == Some(channel_id) && Some(state.user_id) != skip)
            .map(|state| (state.user_id, is_audible(ctx, state)))
            .collect()
    };

    for (user_id, audible) in states {
        if audible {
            start_session(guild_id.get(), user_id.get());
        } else {
            flush_session(services, guild_id.get(), user_id.get()).await?;
        }
    }
    Ok(())
}

pub async fn on_message(
    _ctx: &serenity::Context,
    services: &lumi_kernel::Services,
    message: &serenity::Message,
) -> Result<()> {
    if message.author.bot() {
        return Ok(());
    }
    let Some(guild_id) = message.guild_id else {
        return Ok(());
    };
    // The database cooldown is authoritative, but checking it needs a query on
    // every message. A guild lives on one shard, so a process-local claim can
    // absorb the repeat traffic between awards.
    let key = format!("level:xp:{}:{}", guild_id.get(), message.author.id.get());
    let cooldown = Duration::from_secs(store::DEFAULT_MESSAGE_COOLDOWN_SECS.unsigned_abs());
    if !services.cache.claim_local(&key, cooldown) {
        return Ok(());
    }
    match store::award_xp(
        &services.db,
        guild_id.get(),
        message.author.id.get(),
        DEFAULT_MESSAGE_XP,
        store::DEFAULT_MESSAGE_COOLDOWN_SECS,
    )
    .await
    {
        Ok(Some(_)) => {}
        Ok(None) => services.cache.release_local(&key),
        Err(err) => {
            services.cache.release_local(&key);
            return Err(err);
        }
    }
    Ok(())
}

pub async fn on_voice_state_update(
    ctx: &serenity::Context,
    services: &lumi_kernel::Services,
    old: &Option<VoiceState>,
    new: &VoiceState,
) -> Result<()> {
    let Some(guild_id) = new.guild_id else {
        return Ok(());
    };
    if is_bot(new) {
        return Ok(());
    }
    let user_id = new.user_id;
    let guild = guild_id.get();

    let old_channel = old.as_ref().and_then(|state| state.channel_id);
    let new_channel = new.channel_id;
    let was_audible = old.as_ref().is_some_and(|state| is_audible(ctx, state));
    let now_audible = is_audible(ctx, new);

    if now_audible {
        start_session(guild, user_id.get());
    } else if was_audible {
        flush_session(services, guild, user_id.get()).await?;
    }

    // A join/leave changes whether the *other* members of the channel are alone.
    if let Some(channel_id) = old_channel {
        sync_channel(ctx, services, guild_id, channel_id, Some(user_id)).await?;
    }
    if let Some(channel_id) = new_channel {
        if Some(channel_id) != old_channel {
            sync_channel(ctx, services, guild_id, channel_id, None).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SESSION_MAX_AGE, stale_session};
    use std::time::{Duration, Instant};

    #[test]
    fn only_sessions_older_than_the_cap_are_stale() {
        let now = Instant::now();
        assert!(!stale_session(now, now));
        assert!(!stale_session(now - Duration::from_secs(60), now));
        assert!(!stale_session(now - SESSION_MAX_AGE, now));
        assert!(stale_session(
            now - SESSION_MAX_AGE - Duration::from_secs(1),
            now
        ));
    }
}
