use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use poise::serenity_prelude as serenity;
use serenity::all::{EditMember, UserId};
use tracing::warn;

use lumi_kernel::{Result, Services, prefix};

use crate::shared::format_seconds;

use super::cards::{mention_card, welcome_back_card};
use super::config;
use super::constants::{
    MENTION_COOLDOWN_MS, NICK_COOLDOWN_MS, NICK_PREFIX, NOTICE_TTL, REMOVAL_COOLDOWN_MS,
    WELCOME_COOLDOWN_MS,
};
use super::data::{self, CachedAfk};
use super::mentions::{self, Mention};

/// Message listener: clear the author's AFK, then notify about mentioned AFK users.
pub async fn on_message(
    ctx: &serenity::Context,
    services: &Services,
    message: &serenity::Message,
) -> Result<()> {
    if message.author.bot() {
        return Ok(());
    }
    let Some(guild_id) = message.guild_id else {
        return Ok(());
    };
    let gid = guild_id.get();
    let author = message.author.id.get();

    let map = data::guild_map(&services.cache, &services.db, gid).await?;
    if let Some(entry) = map.get(&author) {
        let on_grace =
            (Utc::now().fixed_offset() - entry.since).num_milliseconds() < REMOVAL_COOLDOWN_MS as i64;
        if !on_grace && !is_command_message(ctx, services, message).await {
            clear_afk(ctx, services, message, entry.since).await?;
        }
    }

    if message.mentions.is_empty() {
        return Ok(());
    }
    notify_mentioned(ctx, services, message, &map).await
}

/// A prefix (or bot-mention) invocation must not count as "back from AFK".
async fn is_command_message(
    ctx: &serenity::Context,
    services: &Services,
    message: &serenity::Message,
) -> bool {
    let Some(guild_id) = message.guild_id else {
        return false;
    };
    if let Ok(prefix) = prefix::get(&services.cache, &services.db, guild_id.get()).await {
        if !prefix.is_empty() && message.content.starts_with(&prefix) {
            return true;
        }
    }
    let mention = format!("<@{}>", ctx.cache.current_user().id.get());
    message.content.starts_with(&mention)
}

/// Delete the AFK row, drop the nickname prefix, then post the welcome card.
async fn clear_afk(
    ctx: &serenity::Context,
    services: &Services,
    message: &serenity::Message,
    since: chrono::DateTime<chrono::FixedOffset>,
) -> Result<()> {
    let Some(guild_id) = message.guild_id else {
        return Ok(());
    };
    let channel_id = message.channel_id;
    let gid = guild_id.get();
    let author = message.author.id.get();

    data::clear(&services.cache, &services.db, gid, author).await?;
    strip_nick_prefix(ctx, services, message, author).await;

    if !config::welcome_enabled(&services.cache, &services.db, gid).await? {
        mentions::clear(&services.cache, gid, author).await?;
        return Ok(());
    }

    let welcome_key = format!("afk:welcome:{}:{author}", channel_id.get());
    if !services
        .cache
        .claim_local(&welcome_key, Duration::from_millis(WELCOME_COOLDOWN_MS))
    {
        return Ok(());
    }

    let duration = format_seconds((Utc::now().fixed_offset() - since).num_seconds());
    let known = mentions::list(&services.cache, gid, author).await?;
    let card = welcome_back_card(
        &duration,
        known.len() as u64,
        Some(format!("afk:mentions:{author}:0")),
    );
    let ttl = config::welcome_ttl_secs(&services.cache, &services.db, gid).await?;

    match channel_id
        .send_message(&ctx.http, card.message().reference_message(message))
        .await
    {
        Ok(sent) => {
            let http = ctx.http.clone();
            let cache = Arc::clone(&services.cache);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_secs(ttl)).await;
                if let Err(error) = sent
                    .delete(&http, Some("AFK welcome back expired"))
                    .await
                {
                    tracing::debug!(%error, "afk welcome delete failed");
                }
                if let Err(error) = mentions::clear(&cache, gid, author).await {
                    warn!(%error, "afk mention cleanup failed");
                }
            });
        }
        Err(error) => {
            warn!(%error, "afk welcome back send failed");
            mentions::clear(&services.cache, gid, author).await?;
        }
    }
    Ok(())
}

/// Remove a leading `[AFK] ` from the member's nickname, rate limited per user.
async fn strip_nick_prefix(
    ctx: &serenity::Context,
    services: &Services,
    message: &serenity::Message,
    user_id: u64,
) {
    let Some(guild_id) = message.guild_id else {
        return;
    };
    let Some(member) = message.member.as_deref() else {
        return;
    };
    let Some(nick) = member.nick.as_ref().map(ToString::to_string) else {
        return;
    };
    let Some(rest) = nick.strip_prefix(NICK_PREFIX) else {
        return;
    };
    let rest = rest.trim();
    let key = format!("afk:nick:{user_id}");
    if !services
        .cache
        .claim_local(&key, Duration::from_millis(NICK_COOLDOWN_MS))
    {
        return;
    }
    let edit = EditMember::new().nickname(rest);
    if let Err(error) = guild_id
        .edit_member(&ctx.http, UserId::new(user_id), edit)
        .await
    {
        tracing::debug!(%error, user_id, "afk nickname strip failed");
    }
}

/// Record the mention for every AFK target, then post one "is AFK" notice.
async fn notify_mentioned(
    ctx: &serenity::Context,
    services: &Services,
    message: &serenity::Message,
    map: &HashMap<u64, CachedAfk>,
) -> Result<()> {
    let Some(guild_id) = message.guild_id else {
        return Ok(());
    };
    let gid = guild_id.get();
    let author = message.author.id.get();

    let targets: Vec<u64> = message
        .mentions
        .iter()
        .filter(|user| !user.bot() && user.id.get() != author)
        .map(|user| user.id.get())
        .collect();
    if targets.is_empty() {
        return Ok(());
    }

    let rows: Vec<(u64, &CachedAfk)> = targets
        .iter()
        .filter_map(|user_id| map.get(user_id).map(|entry| (*user_id, entry)))
        .collect();
    if rows.is_empty() {
        return Ok(());
    }

    let author_name = message
        .member
        .as_deref()
        .and_then(|member| member.nick.as_ref().map(ToString::to_string))
        .unwrap_or_else(|| message.author.name.to_string());
    let mention = Mention {
        author_id: author,
        author_name,
        channel_id: message.channel_id.get(),
        message_id: message.id.get(),
        ts: message.timestamp.unix_timestamp(),
    };

    let batch: Vec<(u64, Mention)> = rows
        .iter()
        .map(|(user_id, _)| (*user_id, mention.clone()))
        .collect();
    mentions::add_batch(&services.cache, gid, &batch).await?;

    let notice_key = format!("afk:notice:{}", message.channel_id.get());
    if !services
        .cache
        .claim_local(&notice_key, Duration::from_millis(MENTION_COOLDOWN_MS))
    {
        return Ok(());
    }

    let Some((target, entry)) = targets
        .iter()
        .find_map(|user_id| map.get(user_id).map(|entry| (*user_id, entry)))
    else {
        return Ok(());
    };

    let name = display_name(ctx, gid, target).filter(|name| !name.is_empty());
    let card = mention_card(
        &name.unwrap_or_else(|| target.to_string()),
        &data::sanitize_reason(&entry.reason),
        &format_seconds((Utc::now().fixed_offset() - entry.since).num_seconds()),
    );

    match message
        .channel_id
        .send_message(&ctx.http, card.message().reference_message(message))
        .await
    {
        Ok(sent) => {
            let http = ctx.http.clone();
            tokio::spawn(async move {
                tokio::time::sleep(NOTICE_TTL).await;
                if let Err(error) = sent.delete(&http, Some("AFK notice expired")).await {
                    tracing::debug!(%error, "afk notice delete failed");
                }
            });
        }
        Err(error) => warn!(%error, "afk notice send failed"),
    }
    Ok(())
}

/// Display name from the cache, with any `[AFK] ` prefix removed.
fn display_name(ctx: &serenity::Context, guild_id: u64, user_id: u64) -> Option<String> {
    let guild = ctx.cache.guild(serenity::GuildId::new(guild_id))?;
    let member = guild.members.get(&UserId::new(user_id))?;
    let name = member.display_name();
    Some(name.strip_prefix(NICK_PREFIX).unwrap_or(name).to_string())
}

