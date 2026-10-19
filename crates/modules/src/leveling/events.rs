use std::sync::OnceLock;
use std::time::Instant;

use poise::serenity_prelude as serenity;
use serenity::all::{ChannelId, GuildId, VoiceState};

use lumi_kernel::dashmap::DashMap;
use lumi_kernel::Result;

use super::store;

const DEFAULT_MESSAGE_XP: i64 = 15;
const VOICE_XP_PER_MINUTE: i64 = 5;

static VOICE_SESSIONS: OnceLock<DashMap<(u64, u64), Instant>> = OnceLock::new();

fn voice_sessions() -> &'static DashMap<(u64, u64), Instant> {
    VOICE_SESSIONS.get_or_init(DashMap::new)
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
    store::award_xp(
        &services.db,
        guild_id.get(),
        message.author.id.get(),
        DEFAULT_MESSAGE_XP,
        store::DEFAULT_MESSAGE_COOLDOWN_SECS,
    )
    .await?;
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

    let was = old.as_ref().is_some_and(|state| is_audible(ctx, state));
    let is = is_audible(ctx, new);
    if was == is {
        return Ok(());
    }

    let sessions = voice_sessions();
    let key = (guild_id.get(), user_id.get());
    if is {
        sessions.insert(key, Instant::now());
        return Ok(());
    }

    let Some((_, started)) = sessions.remove(&key) else {
        return Ok(());
    };
    let minutes = (started.elapsed().as_secs() / 60) as i32;
    if minutes < 1 {
        return Ok(());
    }
    store::add_voice_minutes(
        &services.db,
        guild_id.get(),
        user_id.get(),
        minutes,
        i64::from(minutes) * VOICE_XP_PER_MINUTE,
    )
    .await?;
    Ok(())
}
