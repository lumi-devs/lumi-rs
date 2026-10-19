use poise::serenity_prelude as serenity;
use serenity::all::UserId;

use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::Card;

use super::{store, views};

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

    let card = match account {
        Some(acc) => {
            let rank = store::get_user_rank(&ctx.data().db, guild_id, target_id).await?;
            let next_xp = store::xp_for_level(acc.level);
            let user_obj = target_user_id.to_user(ctx.http()).await.ok();
            let username = user_obj
                .as_ref()
                .map_or_else(|| format!("User {target_id}"), |u| u.name.to_string());
            let avatar_url = user_obj.as_ref().map(|u| u.face());

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
        }
        None => {
            let user_obj = target_user_id.to_user(ctx.http()).await.ok();
            let username = user_obj
                .as_ref()
                .map_or_else(|| format!("User {target_id}"), |u| u.name.to_string());
            views::no_rank_card(&username)
        }
    };

    ctx.send(card.reply()).await?;
    Ok(())
}

/// View the top 10 server members on the XP leaderboard.
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let entries = store::get_leaderboard(&ctx.data().db, guild_id, 10).await?;
    let card = views::leaderboard_card(&entries);
    ctx.send(card.reply()).await?;
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
