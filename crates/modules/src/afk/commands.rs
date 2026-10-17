use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::{Card, CardOptions, make_list_card};

use super::data;

#[poise::command(slash_command)]
pub async fn afk(
    ctx: Context<'_>,
    #[description = "Reason for being AFK"] reason: Option<String>,
) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild_id() else {
        ctx.say("Use this in a server.").await?;
        return Ok(());
    };
    let reason = reason.unwrap_or_else(|| "AFK".to_string());
    data::set(&ctx.data().db, guild_id.get(), ctx.author().id.get(), &reason).await?;
    let card = Card::success("AFK set", format!("You are now AFK: **{reason}**"));
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

#[poise::command(slash_command)]
pub async fn afklist(ctx: Context<'_>) -> Result<(), Error> {
    let Some(guild_id) = ctx.guild_id() else {
        ctx.say("Use this in a server.").await?;
        return Ok(());
    };
    let rows = data::list(&ctx.data().db, guild_id.get()).await?;
    let items: Vec<String> = rows
        .iter()
        .map(|row| format!("<@{}> \u{2014} {}", row.user_id, row.reason))
        .collect();
    ctx.send(make_list_card("AFK Users", &items, CardOptions::default()).reply())
        .await?;
    Ok(())
}

#[poise::command(slash_command)]
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
