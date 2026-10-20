use std::time::Duration;

use chrono::Utc;
use poise::serenity_prelude as serenity;
use serenity::all::EditMember;

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::{Card, CardOptions, make_list_card};

use crate::shared::format_seconds;

use super::cards;
use super::config;
use super::constants::{MAX_NICKNAME, NICK_COOLDOWN_MS, NICK_PREFIX};
use super::data;

#[poise::command(slash_command, prefix_command)]
pub async fn afk(
    ctx: Context<'_>,
    #[description = "Reason for being AFK"] reason: Option<String>,
) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild_id() else {
        ctx.say("Use this in a server.").await?;
        return Ok(());
    };
    let gid = guild_id.get();
    let user_id = ctx.author().id;

    let reason = data::sanitize_reason(reason.as_deref().unwrap_or("AFK"));
    let existing = data::get(&ctx.data().db, gid, user_id.get()).await?;
    data::set(
        &ctx.data().cache,
        &ctx.data().db,
        gid,
        user_id.get(),
        &reason,
    )
    .await?;

    if existing.is_none() {
        apply_nick_prefix(ctx, guild_id, user_id).await?;
    }

    let (title, body) = match &existing {
        Some(entry) if entry.reason == reason => (
            "\u{1f4a4} Already AFK",
            format!("You are already AFK with the reason: **{reason}**"),
        ),
        Some(_) => (
            "\u{270f}\u{fe0f} AFK Updated",
            format!("AFK reason updated to: **{reason}**"),
        ),
        None => (
            "\u{1f4a4} AFK Set",
            format!("You are now AFK: **{reason}**"),
        ),
    };
    ctx.send(cards::status_card(title, &body).reply()).await?;
    Ok(())
}

/// Prefix the current nickname with `[AFK] `, once the guild allows it.
async fn apply_nick_prefix(
    ctx: Context<'_>,
    guild_id: serenity::GuildId,
    user_id: serenity::UserId,
) -> Result<(), Error> {
    if !config::nick_prefix_enabled(&ctx.data().cache, &ctx.data().db, guild_id.get()).await? {
        return Ok(());
    }
    let Some(member) = ctx.author_member().await else {
        return Ok(());
    };
    let name = member.display_name().to_string();
    if name.starts_with(NICK_PREFIX) {
        return Ok(());
    }
    let key = format!("afk:nick:{}", user_id.get());
    if !ctx
        .data()
        .cache
        .claim_local(&key, Duration::from_millis(NICK_COOLDOWN_MS))
    {
        return Ok(());
    }
    let nick = crate::shared::truncate_chars(&format!("{NICK_PREFIX}{name}"), MAX_NICKNAME);
    if let Err(error) = guild_id
        .edit_member(ctx.http(), user_id, EditMember::new().nickname(nick))
        .await
    {
        tracing::debug!(%error, user_id = user_id.get(), "afk nickname prefix failed");
    }
    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub async fn afklist(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild_id() else {
        ctx.say("Use this in a server.").await?;
        return Ok(());
    };
    let rows = data::list(&ctx.data().db, guild_id.get()).await?;
    let now = Utc::now().fixed_offset();
    let items: Vec<String> = rows
        .iter()
        .map(|row| {
            format!(
                "<@{}> \u{2014} {} (for {})",
                row.user_id.get(),
                row.reason,
                format_seconds((now - row.since).num_seconds())
            )
        })
        .collect();
    ctx.send(make_list_card("AFK Users", &items, CardOptions::default()).reply())
        .await?;
    Ok(())
}

#[poise::command(slash_command, prefix_command)]
pub async fn afkstats(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild_id() else {
        ctx.say("Use this in a server.").await?;
        return Ok(());
    };
    let total = data::count(&ctx.data().db, guild_id.get()).await?;
    let card = Card::info("AFK Stats", format!("**{total}** member(s) currently AFK"));
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

pub fn commands() -> Vec<Command> {
    vec![afk(), afklist(), afkstats()]
}
