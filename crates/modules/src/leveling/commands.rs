use poise::serenity_prelude as serenity;
use serenity::all::UserId;

use lumi_kernel::bot::{Command, Context, Error};

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

pub fn commands() -> Vec<Command> {
    vec![rank(), leaderboard()]
}
