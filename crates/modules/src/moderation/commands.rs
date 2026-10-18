use poise::serenity_prelude as serenity;
use serenity::all::UserId;

use lumi_db::entities::enums::CaseAction;
use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::Card;

use super::store::{self, CreateCaseParams};
use super::views::{case_card, mod_action_success_card, notes_list_card};

/// Warn a guild member.
#[poise::command(slash_command, guild_only, default_member_permissions = "MODERATE_MEMBERS")]
pub async fn warn(
    ctx: Context<'_>,
    #[description = "The target user"] user: UserId,
    #[description = "Reason for warning"] reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let mod_id = ctx.author().id.get();

    let case = store::create_case(
        &ctx.data().db,
        CreateCaseParams {
            guild_id,
            user_id: user.get(),
            moderator_id: mod_id,
            action: CaseAction::Warn,
            reason: reason.as_deref(),
            duration_seconds: None,
        },
    )
    .await?;

    let card = mod_action_success_card(&CaseAction::Warn, user.get(), case.case_number, reason.as_deref());
    ctx.send(card.reply()).await?;
    Ok(())
}

/// Kick a guild member.
#[poise::command(slash_command, guild_only, default_member_permissions = "KICK_MEMBERS")]
pub async fn kick(
    ctx: Context<'_>,
    #[description = "The target user"] user: UserId,
    #[description = "Reason for kick"] reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let mod_id = ctx.author().id.get();

    // Execute Discord kick via guild.kick
    guild.kick(ctx.http(), user, reason.as_deref()).await?;

    let case = store::create_case(
        &ctx.data().db,
        CreateCaseParams {
            guild_id: guild.get(),
            user_id: user.get(),
            moderator_id: mod_id,
            action: CaseAction::Kick,
            reason: reason.as_deref(),
            duration_seconds: None,
        },
    )
    .await?;

    let card = mod_action_success_card(&CaseAction::Kick, user.get(), case.case_number, reason.as_deref());
    ctx.send(card.reply()).await?;
    Ok(())
}

/// Ban a user from the guild.
#[poise::command(slash_command, guild_only, default_member_permissions = "BAN_MEMBERS")]
pub async fn ban(
    ctx: Context<'_>,
    #[description = "The target user"] user: UserId,
    #[description = "Reason for ban"] reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let mod_id = ctx.author().id.get();

    guild.ban(ctx.http(), user, 0, reason.as_deref()).await?;

    let case = store::create_case(
        &ctx.data().db,
        CreateCaseParams {
            guild_id: guild.get(),
            user_id: user.get(),
            moderator_id: mod_id,
            action: CaseAction::Ban,
            reason: reason.as_deref(),
            duration_seconds: None,
        },
    )
    .await?;

    let card = mod_action_success_card(&CaseAction::Ban, user.get(), case.case_number, reason.as_deref());
    ctx.send(card.reply()).await?;
    Ok(())
}

/// Unban a user from the guild.
#[poise::command(slash_command, guild_only, default_member_permissions = "BAN_MEMBERS")]
pub async fn unban(
    ctx: Context<'_>,
    #[description = "The target user id"] user: UserId,
    #[description = "Reason for unban"] reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let mod_id = ctx.author().id.get();

    guild.unban(ctx.http(), user, reason.as_deref()).await?;

    let case = store::create_case(
        &ctx.data().db,
        CreateCaseParams {
            guild_id: guild.get(),
            user_id: user.get(),
            moderator_id: mod_id,
            action: CaseAction::Unban,
            reason: reason.as_deref(),
            duration_seconds: None,
        },
    )
    .await?;

    let card = mod_action_success_card(&CaseAction::Unban, user.get(), case.case_number, reason.as_deref());
    ctx.send(card.reply()).await?;
    Ok(())
}

/// Inspect a specific moderation case.
#[poise::command(slash_command, guild_only, default_member_permissions = "MODERATE_MEMBERS")]
pub async fn case(
    ctx: Context<'_>,
    #[description = "Case number to lookup"] number: i32,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();

    let case_opt = store::get_case(&ctx.data().db, guild_id, number).await?;
    match case_opt {
        Some(c) => {
            let card = case_card(&c);
            ctx.send(card.reply()).await?;
        }
        None => {
            let card = Card::lines("🔍 Case Not Found", vec![format!("Case `#{}` does not exist in this server.", number)]);
            ctx.send(card.into_reply().reply()).await?;
        }
    }
    Ok(())
}

/// Update the reason of an existing moderation case.
#[poise::command(slash_command, guild_only, default_member_permissions = "MODERATE_MEMBERS")]
pub async fn reason(
    ctx: Context<'_>,
    #[description = "Case number"] number: i32,
    #[description = "New reason"] reason: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();

    let updated = store::update_case_reason(&ctx.data().db, guild_id, number, &reason).await?;
    if updated {
        let card = Card::lines("✅ Reason Updated", vec![format!("Updated reason for Case `#{}` to: **{}**", number, reason)]);
        ctx.send(card.into_reply().reply()).await?;
    } else {
        let card = Card::lines("❌ Case Not Found", vec![format!("Case `#{}` does not exist.", number)]);
        ctx.send(card.into_reply().reply()).await?;
    }
    Ok(())
}

/// View or add moderator notes for a member.
#[poise::command(slash_command, guild_only, default_member_permissions = "MODERATE_MEMBERS")]
pub async fn note(
    ctx: Context<'_>,
    #[description = "Target user"] user: UserId,
    #[description = "Note text to append (leave empty to view notes)"] text: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?.get();
    let mod_id = ctx.author().id.get();

    if let Some(content) = text {
        store::add_note(&ctx.data().db, guild_id, user.get(), mod_id, &content).await?;
        let card = Card::lines("📝 Note Added", vec![format!("Added note to <@{}>: **{}**", user.get(), content)]);
        ctx.send(card.into_reply().reply()).await?;
    } else {
        let notes = store::list_notes(&ctx.data().db, guild_id, user.get()).await?;
        let card = notes_list_card(user.get(), &notes);
        ctx.send(card.reply()).await?;
    }
    Ok(())
}

pub fn commands() -> Vec<Command> {
    vec![warn(), kick(), ban(), unban(), case(), reason(), note()]
}
