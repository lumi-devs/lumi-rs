use poise::serenity_prelude as serenity;
use serenity::all::{CreateAttachment, UserId};

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::Card;

use super::{images, store, views};

/// View your rank and XP progress, or check another member's.
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn rank(
    ctx: Context<'_>,
    #[description = "Target user"] user: Option<UserId>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let target_user_id = user.unwrap_or_else(|| ctx.author().id);
    let target_id = target_user_id.get();

    let account = store::get_account(&ctx.data().db, guild_id, target_id).await?;
    let user_obj = target_user_id.to_user(ctx.http()).await.ok();
    let username = user_obj
        .as_ref()
        .map_or_else(|| format!("User {target_id}"), |u| u.name.to_string());
    let avatar_url = user_obj.as_ref().map(|u| u.face());

    let Some(acc) = account else {
        ctx.send(views::no_rank_card(&username).reply()).await?;
        return Ok(());
    };

    let rank = store::get_user_rank(&ctx.data().db, guild_id, target_id).await?;
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
            let attachment = CreateAttachment::bytes(png, "rank.png");
            ctx.send(poise::CreateReply::default().attachment(attachment)).await?;
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
#[poise::command(slash_command, prefix_command, guild_only, rename = "levelboard", aliases("xplb", "xpboard"))]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let entries = store::get_leaderboard(&ctx.data().db, guild_id, 10).await?;

    if entries.is_empty() {
        ctx.send(views::leaderboard_card(&entries).reply()).await?;
        return Ok(());
    }

    let mut rows = Vec::with_capacity(entries.len());
    let top_xp = entries.first().map_or(1, |e| e.total_xp.max(1));
    for (idx, entry) in entries.iter().enumerate() {
        let u_id = UserId::new(entry.user_id.parse::<u64>().unwrap_or(0));
        let name = if let Ok(u) = u_id.to_user(ctx.http()).await {
            u.name.to_string()
        } else {
            format!("User {}", entry.user_id)
        };
        rows.push(images::LeaderboardRow {
            rank: idx + 1,
            username: name,
            level: entry.level,
            xp: entry.total_xp,
            progress: entry.total_xp as f64 / top_xp as f64,
        });
    }

    let guild_name = ctx.guild().map_or_else(|| "Server".to_string(), |g| g.name.to_string());
    let title = format!("🏆 Leaderboard — {guild_name}");
    let subtitle = "Top ranked members by XP";

    match images::render_leaderboard_card(&title, subtitle, &rows, "#6366f1") {
        Ok(png) => {
            let attachment = CreateAttachment::bytes(png, "leaderboard.png");
            ctx.send(poise::CreateReply::default().attachment(attachment)).await?;
        }
        Err(err) => {
            tracing::warn!(%err, "failed to render leaderboard PNG, falling back to text view");
            ctx.send(views::leaderboard_card(&entries).reply()).await?;
        }
    }

    Ok(())
}

async fn display_name(ctx: Context<'_>, user: UserId) -> String {
    user.to_user(ctx.http())
        .await
        .ok()
        .map_or_else(|| format!("<@{}>", user.get()), |u| u.name.to_string())
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
    if kind != "xp" && kind != "level" {
        ctx.send(invalid_kind_card().reply()).await?;
        return Ok(());
    }
    if amount < 1 {
        let card = Card::error("Level Update Failed", "Amount must be `1` or higher.");
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let account = store::add_xp_or_level(&ctx.data().db, guild_id, user.get(), &kind, amount).await?;
    let unit = if kind == "xp" { "XP" } else { "Levels" };
    let name = display_name(ctx, user).await;
    let card = Card::success(
        format!("{unit} Granted"),
        format!("{name} is now level **{}** with **{}** XP.", account.level, account.xp),
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
    if kind != "xp" && kind != "level" {
        ctx.send(invalid_kind_card().reply()).await?;
        return Ok(());
    }
    if amount < 1 {
        let card = Card::error("Level Update Failed", "Amount must be `1` or higher.");
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let account =
        store::remove_xp_or_level(&ctx.data().db, guild_id, user.get(), &kind, amount).await?;
    let unit = if kind == "xp" { "XP" } else { "Levels" };
    let name = display_name(ctx, user).await;
    let card = Card::success(
        format!("{unit} Removed"),
        format!("{name} is now level **{}** with **{}** XP.", account.level, account.xp),
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
    if level < 1 {
        let card = Card::error("Level Update Failed", "Level must be `1` or higher.");
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let account = store::set_level_and_xp(&ctx.data().db, guild_id, user.get(), level, xp).await?;
    let name = display_name(ctx, user).await;
    let card = Card::success(
        "Level Updated",
        format!("{name} is now level **{}** with **{}** XP.", account.level, account.xp),
    );
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

pub fn commands() -> Vec<Command> {
    vec![rank(), leaderboard(), level()]
}
