use poise::serenity_prelude as serenity;
use serenity::all::{GuildId, UserId};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::Card;

use crate::shared::{ellipsize, require_guild};

use super::store::{self, XpKind};
use super::{images, views};

/// Prefer the cached guild display name; never blocks on a REST round-trip.
fn cached_name(ctx: Context<'_>, guild_id: GuildId, user_id: UserId) -> Option<String> {
    ctx.cache().guild(guild_id).and_then(|guild| {
        guild
            .members
            .get(&user_id)
            .map(|m| m.display_name().to_string())
    })
}

async fn display_name(ctx: Context<'_>, guild_id: GuildId, user: UserId) -> String {
    if let Some(name) = cached_name(ctx, guild_id, user) {
        return name;
    }
    if let Ok(member) = guild_id.member(ctx.http(), user).await {
        return member.display_name().to_string();
    }
    user.to_user(ctx.http()).await.ok().map_or_else(
        || format!("<@{}>", user.get()),
        |u| u.display_name().to_string(),
    )
}

/// Guild display name plus avatar, preferring the cache, then one REST lookup.
async fn identity(ctx: Context<'_>, guild_id: GuildId, user: UserId) -> (String, Option<String>) {
    if let Some(member) = ctx
        .cache()
        .guild(guild_id)
        .and_then(|guild| guild.members.get(&user).cloned())
    {
        return (member.display_name().to_string(), Some(member.user.face()));
    }
    if let Ok(member) = guild_id.member(ctx.http(), user).await {
        return (member.display_name().to_string(), Some(member.user.face()));
    }
    match user.to_user(ctx.http()).await {
        Ok(user) => (user.display_name().to_string(), Some(user.face())),
        Err(_) => (format!("<@{}>", user.get()), None),
    }
}

/// View your rank and XP progress, or check another member's.
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn rank(
    ctx: Context<'_>,
    #[description = "Target user"] user: Option<UserId>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?;
    let target_user_id = user.unwrap_or_else(|| ctx.author().id);
    let target_id = target_user_id.get();

    let account = store::get_account(&ctx.data().db, guild_id.get(), target_id).await?;
    let (username, avatar_url) = identity(ctx, guild_id, target_user_id).await;

    let Some(acc) = account else {
        ctx.send(views::no_rank_card(&username).reply()).await?;
        return Ok(());
    };

    let rank = store::get_user_rank(ctx.data().reader(), guild_id.get(), target_id).await?;
    let next_xp = store::xp_for_level(acc.level);

    let png_res = images::render_rank_card(images::RankCardData {
        username: &username,
        avatar_url: avatar_url.as_deref(),
        level: acc.level,
        rank,
        current_xp: acc.xp,
        next_level_xp: next_xp,
        message_count: acc.message_count,
        voice_minutes: acc.voice_minutes,
        accent: "#6366f1",
    })
    .await;

    match png_res {
        Ok(png) => {
            let attachment = serenity::CreateAttachment::bytes(png, "rank.png");
            ctx.send(poise::CreateReply::default().attachment(attachment))
                .await?;
        }
        Err(err) => {
            tracing::warn!(%err, "failed to render rank card PNG, falling back to text view");
            ctx.send(
                views::rank_card(
                    &username,
                    avatar_url,
                    acc.level,
                    rank,
                    acc.xp,
                    next_xp,
                    acc.total_xp,
                    acc.message_count,
                    acc.voice_minutes,
                )
                .reply(),
            )
            .await?;
        }
    }

    Ok(())
}

/// View the top 10 server members on the XP leaderboard.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    rename = "levelboard",
    aliases("xplb", "xpboard")
)]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?;
    let entries = store::get_leaderboard(ctx.data().reader(), guild_id.get(), 10, 0).await?;

    if entries.is_empty() {
        ctx.send(views::leaderboard_card(&entries).reply()).await?;
        return Ok(());
    }

    let mut rows = Vec::with_capacity(entries.len());
    let top_xp = entries.first().map_or(1, |e| e.total_xp.max(1));
    for (idx, entry) in entries.iter().enumerate() {
        let user_id = Some(UserId::new(entry.user_id.get()));
        let name = user_id
            .and_then(|id| cached_name(ctx, guild_id, id))
            .unwrap_or_else(|| format!("User {}", entry.user_id.get()));
        rows.push(images::LeaderboardRow {
            rank: idx + 1,
            username: ellipsize(&name, 22),
            level: entry.level,
            xp: entry.total_xp,
            progress: entry.total_xp as f64 / top_xp as f64,
        });
    }

    let guild_name = ctx
        .guild()
        .map_or_else(|| "Server".to_string(), |g| g.name.to_string());
    let title = format!("🏆 Leaderboard — {guild_name}");
    let subtitle = "Top ranked members by XP";

    match images::render_leaderboard_card(&title, subtitle, &rows, "#6366f1") {
        Ok(png) => {
            let attachment = serenity::CreateAttachment::bytes(png, "leaderboard.png");
            ctx.send(poise::CreateReply::default().attachment(attachment))
                .await?;
        }
        Err(err) => {
            tracing::warn!(%err, "failed to render leaderboard PNG, falling back to text view");
            ctx.send(views::leaderboard_card(&entries).reply()).await?;
        }
    }

    Ok(())
}

fn invalid_kind_card() -> lumi_ui::CardReply {
    Card::error(
        "Level Update Failed",
        "Kind must be `\"xp\"` or `\"level\"`.",
    )
    .into_reply()
}

/// Adjust member XP and levels (staff only).
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MANAGE_GUILD",
    subcommands("level_add", "level_remove", "level_set")
)]
pub async fn level(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let card = Card::info(
        "Level Management",
        "Use `/level add`, `/level remove`, or `/level set` to adjust member XP and levels.",
    );
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Grant XP or levels to a member (staff only).
#[poise::command(slash_command, prefix_command, rename = "add")]
pub async fn level_add(
    ctx: Context<'_>,
    #[description = "Who receives it."] user: UserId,
    #[description = "Grant `xp` or whole `level`s."] kind: String,
    #[description = "How much to grant."]
    #[min = 1]
    amount: i64,
) -> Result<(), Error> {
    ctx.defer().await?;
    let Some(kind) = XpKind::parse(&kind) else {
        ctx.send(invalid_kind_card().reply()).await?;
        return Ok(());
    };
    let guild_id = require_guild(ctx)?;
    let account =
        store::add_xp_or_level(&ctx.data().db, guild_id.get(), user.get(), kind, amount).await?;
    let name = display_name(ctx, guild_id, user).await;
    let card = Card::success(
        format!("{} Granted", kind.label()),
        format!(
            "{name} is now level **{}** with **{}** XP.",
            account.level, account.xp
        ),
    );
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Take XP or levels from a member (staff only).
#[poise::command(slash_command, prefix_command, rename = "remove")]
pub async fn level_remove(
    ctx: Context<'_>,
    #[description = "Who loses it."] user: UserId,
    #[description = "Remove `xp` or whole `level`s."] kind: String,
    #[description = "How much to remove."]
    #[min = 1]
    amount: i64,
) -> Result<(), Error> {
    ctx.defer().await?;
    let Some(kind) = XpKind::parse(&kind) else {
        ctx.send(invalid_kind_card().reply()).await?;
        return Ok(());
    };
    let guild_id = require_guild(ctx)?;
    let account =
        store::remove_xp_or_level(&ctx.data().db, guild_id.get(), user.get(), kind, amount).await?;
    let name = display_name(ctx, guild_id, user).await;
    let card = Card::success(
        format!("{} Removed", kind.label()),
        format!(
            "{name} is now level **{}** with **{}** XP.",
            account.level, account.xp
        ),
    );
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Set a member's level and XP (staff only).
#[poise::command(slash_command, prefix_command, rename = "set")]
pub async fn level_set(
    ctx: Context<'_>,
    #[description = "Whose level to set."] user: UserId,
    #[description = "The new level."]
    #[min = 1]
    level: i32,
    #[description = "Progress toward the next level."]
    #[min = 0]
    xp: Option<i64>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = require_guild(ctx)?;
    let account =
        store::set_level_and_xp(&ctx.data().db, guild_id.get(), user.get(), level, xp).await?;
    let name = display_name(ctx, guild_id, user).await;
    let card = Card::success(
        "Level Updated",
        format!(
            "{name} is now level **{}** with **{}** XP.",
            account.level, account.xp
        ),
    );
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

pub fn commands() -> Vec<Command> {
    vec![rank(), leaderboard(), level()]
}
