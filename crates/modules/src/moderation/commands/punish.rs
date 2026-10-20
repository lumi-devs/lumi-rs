//! Warn, kick, ban, mute, softban and quarantine commands.

use std::time::Duration;

use serenity::all::{RoleId, UserId};

use lumi_db::entities::enums::CaseAction;
use lumi_kernel::bot::{Context, Error};
use lumi_ui::{Card, CardKind};

use crate::confirm;
use crate::moderation::actions::{self, BulkOutcome};
use crate::moderation::store::{self, CreateCaseParams, ModConfig};
use crate::moderation::views::{action_color, action_success_card, human_seconds, punishment_card};

use super::{autocomplete_reason, guild_id, parse_duration, parse_target_list, send_card};

async fn notify_outcome(
    ctx: Context<'_>,
    config: &ModConfig,
    outcome: &BulkOutcome,
    reason: Option<&str>,
) {
    for applied in &outcome.applied {
        actions::notify(
            ctx.http(),
            config,
            applied.target,
            outcome.action,
            applied.case.as_ref(),
            reason,
        )
        .await;
    }
}

/// Apply the highest warn threshold reached, recording the escalated case.
async fn apply_warn_escalation(
    ctx: Context<'_>,
    config: &ModConfig,
    guild_id_num: u64,
    target_id: u64,
    total_warns: u64,
) -> Result<Option<String>, Error> {
    let thresholds = store::get_warn_thresholds(&ctx.data().db, guild_id_num).await?;
    let Some(rule) = thresholds
        .iter()
        .filter(|threshold| threshold.warn_count as u64 <= total_warns)
        .max_by_key(|threshold| threshold.warn_count)
    else {
        return Ok(None);
    };

    let guild = guild_id(ctx)?;
    let bot_id = ctx.framework().bot_id().get();
    let reason = format!(
        "Auto: {total_warns} warns reached threshold ({}).",
        rule.warn_count
    );
    let target = UserId::new(target_id);

    let (message, case) = match rule.action {
        CaseAction::Mute => {
            let seconds = rule.duration.unwrap_or(3_600).max(60);
            let duration = Duration::from_secs(u64::try_from(seconds).unwrap_or(3_600));
            actions::apply_timeout(ctx.http(), guild, target, duration, &reason).await?;
            let case = actions::record_case(
                &ctx.data().db,
                guild_id_num,
                target,
                bot_id,
                CaseAction::Mute,
                Some(&reason),
                Some(seconds),
            )
            .await;
            (
                format!(
                    "Automatically timed out for {} (threshold reached)",
                    human_seconds(seconds)
                ),
                case,
            )
        }
        CaseAction::Kick => {
            guild.kick(ctx.http(), target, Some(&reason)).await?;
            let case = actions::record_case(
                &ctx.data().db,
                guild_id_num,
                target,
                bot_id,
                CaseAction::Kick,
                Some(&reason),
                None,
            )
            .await;
            ("Automatically kicked (threshold reached)".to_string(), case)
        }
        CaseAction::Ban => {
            guild.ban(ctx.http(), target, 0, Some(&reason)).await?;
            let case = actions::record_case(
                &ctx.data().db,
                guild_id_num,
                target,
                bot_id,
                CaseAction::Ban,
                Some(&reason),
                None,
            )
            .await;
            ("Automatically banned (threshold reached)".to_string(), case)
        }
        _ => return Ok(None),
    };

    actions::notify(
        ctx.http(),
        config,
        target,
        rule.action,
        case.as_ref(),
        Some(&reason),
    )
    .await;
    Ok(Some(message))
}

/// Warn a guild member.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MODERATE_MEMBERS",
    required_bot_permissions = "MODERATE_MEMBERS"
)]
pub async fn warn(
    ctx: Context<'_>,
    #[description = "The target user"] user: UserId,
    #[description = "Reason for warning"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_id = guild_id(ctx)?.get();
    let config = store::config(&ctx.data().cache, &ctx.data().db, guild_id).await;

    let case = store::create_case(
        &ctx.data().db,
        CreateCaseParams {
            guild_id,
            user_id: user.get(),
            moderator_id: ctx.author().id.get(),
            action: CaseAction::Warn,
            reason: reason.as_deref(),
            duration_seconds: None,
        },
    )
    .await?;

    let total_warns = store::count_warns(&ctx.data().db, guild_id, user.get()).await?;
    let escalation =
        match apply_warn_escalation(ctx, &config, guild_id, user.get(), total_warns).await {
            Ok(escalation) => escalation,
            Err(error) => {
                tracing::error!(%error, guild_id, user_id = user.get(), "warn escalation failed");
                Some("Automatic escalation failed; check the logs.".to_string())
            }
        };

    actions::notify(
        ctx.http(),
        &config,
        user,
        CaseAction::Warn,
        Some(&case),
        reason.as_deref(),
    )
    .await;

    let mut lines = vec![
        format!("Successfully warned <@{}>.", user.get()),
        format!(
            "**Case:** `#{}` · **Total Warns:** `{total_warns}`",
            case.case_number
        ),
    ];
    if let Some(reason) = reason.as_deref() {
        lines.push(format!("**Reason:** {reason}"));
    }
    if let Some(escalation) = escalation {
        lines.push(format!("⚡ **Escalation Action:** {escalation}"));
    }

    send_card(
        ctx,
        Card::lines("⚠️ Member Warned", lines).accent(Some(action_color(&CaseAction::Warn))),
    )
    .await
}

/// Kick a guild member or multiple members.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "KICK_MEMBERS",
    required_bot_permissions = "KICK_MEMBERS"
)]
pub async fn kick(
    ctx: Context<'_>,
    #[description = "The target user"] user: Option<UserId>,
    #[description = "Additional members/IDs/mentions (up to 25)"] members: Option<String>,
    #[description = "Reason for kick"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
) -> Result<(), Error> {
    let targets = parse_target_list(user, members.as_deref());
    if targets.is_empty() {
        return send_card(
            ctx,
            Card::lines(
                "❌ Missing Target",
                vec!["Please specify a member or list of members to kick.".to_string()],
            )
            .kind(CardKind::Error),
        )
        .await;
    }

    let display = targets
        .iter()
        .map(|target| format!("<@{}>", target.get()))
        .collect::<Vec<_>>()
        .join(", ");
    let body = format!(
        "Are you sure you want to kick the following member(s)?\n\n{display}\n\n**Reason**: {}",
        reason.as_deref().unwrap_or("No reason provided")
    );
    if !confirm::confirm(ctx, "moderation", "⚠️ Confirm Kick", &body, "Confirm Kick").await? {
        return send_card(
            ctx,
            Card::lines(
                "Action Cancelled",
                vec!["Kick action was cancelled.".to_string()],
            ),
        )
        .await;
    }

    ctx.defer().await?;
    let guild = guild_id(ctx)?;
    let http = ctx.http();
    let reason_owned = reason.clone();
    let outcome = actions::apply_bulk(
        &ctx.data().db,
        guild.get(),
        ctx.author().id.get(),
        CaseAction::Kick,
        reason.as_deref(),
        targets,
        move |target| {
            let reason = reason_owned.clone();
            async move { guild.kick(http, target, reason.as_deref()).await }
        },
    )
    .await;

    let config = store::config(&ctx.data().cache, &ctx.data().db, guild.get()).await;
    notify_outcome(ctx, &config, &outcome, reason.as_deref()).await;
    ctx.send(punishment_card(&outcome, reason.as_deref()).reply())
        .await?;
    Ok(())
}

/// Ban a user or multiple users from the guild.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "BAN_MEMBERS",
    required_bot_permissions = "BAN_MEMBERS"
)]
pub async fn ban(
    ctx: Context<'_>,
    #[description = "The target user"] user: Option<UserId>,
    #[description = "Additional users/IDs/mentions (up to 25)"] users: Option<String>,
    #[description = "Number of days of messages to delete (0-7)"] delete_days: Option<u8>,
    #[description = "Reason for ban"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
) -> Result<(), Error> {
    let targets = parse_target_list(user, users.as_deref());
    if targets.is_empty() {
        return send_card(
            ctx,
            Card::lines(
                "❌ Missing Target",
                vec!["Please specify a user or list of users to ban.".to_string()],
            )
            .kind(CardKind::Error),
        )
        .await;
    }

    let days = delete_days.unwrap_or(0).min(7);
    let display = targets
        .iter()
        .map(|target| format!("<@{}>", target.get()))
        .collect::<Vec<_>>()
        .join(", ");
    let body = format!(
        "Are you sure you want to ban the following user(s)?\n\n{display}\n\n**Delete Messages**: {days} day(s)\n**Reason**: {}",
        reason.as_deref().unwrap_or("No reason provided")
    );
    if !confirm::confirm(ctx, "moderation", "⚠️ Confirm Ban", &body, "Confirm Ban").await? {
        return send_card(
            ctx,
            Card::lines(
                "Action Cancelled",
                vec!["Ban action was cancelled.".to_string()],
            ),
        )
        .await;
    }

    ctx.defer().await?;
    let guild = guild_id(ctx)?;
    let http = ctx.http();
    let reason_owned = reason.clone();
    let outcome = actions::apply_bulk(
        &ctx.data().db,
        guild.get(),
        ctx.author().id.get(),
        CaseAction::Ban,
        reason.as_deref(),
        targets,
        move |target| {
            let reason = reason_owned.clone();
            async move {
                guild
                    .ban(http, target, u32::from(days), reason.as_deref())
                    .await
            }
        },
    )
    .await;

    let config = store::config(&ctx.data().cache, &ctx.data().db, guild.get()).await;
    notify_outcome(ctx, &config, &outcome, reason.as_deref()).await;
    ctx.send(punishment_card(&outcome, reason.as_deref()).reply())
        .await?;
    Ok(())
}

/// Unban a user from the guild.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "BAN_MEMBERS",
    required_bot_permissions = "BAN_MEMBERS"
)]
pub async fn unban(
    ctx: Context<'_>,
    #[description = "The target user id"] user: UserId,
    #[description = "Reason for unban"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = guild_id(ctx)?;
    guild.unban(ctx.http(), user, reason.as_deref()).await?;

    let config = store::config(&ctx.data().cache, &ctx.data().db, guild.get()).await;
    let case = actions::record_case(
        &ctx.data().db,
        guild.get(),
        user,
        ctx.author().id.get(),
        CaseAction::Unban,
        reason.as_deref(),
        None,
    )
    .await;
    actions::notify(
        ctx.http(),
        &config,
        user,
        CaseAction::Unban,
        case.as_ref(),
        reason.as_deref(),
    )
    .await;

    ctx.send(action_success_card(&CaseAction::Unban, user.get(), case, reason.as_deref()).reply())
        .await?;
    Ok(())
}

/// Timeout / mute a member in the server.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MODERATE_MEMBERS",
    required_bot_permissions = "MODERATE_MEMBERS"
)]
pub async fn mute(
    ctx: Context<'_>,
    #[description = "The target user"] user: UserId,
    #[description = "Duration e.g. 10m, 1h, 1d (max 28d)"] duration: String,
    #[description = "Reason for mute"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let Some(duration) = parse_duration(&duration) else {
        return send_card(
            ctx,
            Card::lines(
                "❌ Invalid Duration",
                vec!["Use formats like `10m`, `2h`, or `1d` (max 28d).".to_string()],
            )
            .kind(CardKind::Error),
        )
        .await;
    };
    if duration.is_zero() {
        return send_card(
            ctx,
            Card::lines(
                "❌ Invalid Duration",
                vec!["Duration must be greater than zero.".to_string()],
            )
            .kind(CardKind::Error),
        )
        .await;
    }

    let guild = guild_id(ctx)?;
    let duration = duration.min(actions::MAX_TIMEOUT);
    let seconds = i32::try_from(duration.as_secs()).unwrap_or(i32::MAX);
    actions::apply_timeout(
        ctx.http(),
        guild,
        user,
        duration,
        reason.as_deref().unwrap_or("No reason provided"),
    )
    .await?;

    let config = store::config(&ctx.data().cache, &ctx.data().db, guild.get()).await;
    let case = actions::record_case(
        &ctx.data().db,
        guild.get(),
        user,
        ctx.author().id.get(),
        CaseAction::Mute,
        reason.as_deref(),
        Some(seconds),
    )
    .await;
    actions::notify(
        ctx.http(),
        &config,
        user,
        CaseAction::Mute,
        case.as_ref(),
        reason.as_deref(),
    )
    .await;

    ctx.send(action_success_card(&CaseAction::Mute, user.get(), case, reason.as_deref()).reply())
        .await?;
    Ok(())
}

/// Remove timeout / unmute a member in the server.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MODERATE_MEMBERS",
    required_bot_permissions = "MODERATE_MEMBERS"
)]
pub async fn unmute(
    ctx: Context<'_>,
    #[description = "The target user"] user: UserId,
    #[description = "Reason for unmute"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = guild_id(ctx)?;
    actions::clear_timeout(
        ctx.http(),
        guild,
        user,
        reason.as_deref().unwrap_or("No reason provided"),
    )
    .await?;

    let config = store::config(&ctx.data().cache, &ctx.data().db, guild.get()).await;
    let case = actions::record_case(
        &ctx.data().db,
        guild.get(),
        user,
        ctx.author().id.get(),
        CaseAction::Unmute,
        reason.as_deref(),
        None,
    )
    .await;
    actions::notify(
        ctx.http(),
        &config,
        user,
        CaseAction::Unmute,
        case.as_ref(),
        reason.as_deref(),
    )
    .await;

    ctx.send(action_success_card(&CaseAction::Unmute, user.get(), case, reason.as_deref()).reply())
        .await?;
    Ok(())
}

/// Softban a member (ban then immediately unban to purge recent messages).
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "BAN_MEMBERS",
    required_bot_permissions = "BAN_MEMBERS"
)]
pub async fn softban(
    ctx: Context<'_>,
    #[description = "The target user"] target: UserId,
    #[description = "Days of messages to delete (1-7)"]
    #[min = 1]
    #[max = 7]
    days: Option<u8>,
    #[description = "Reason for softban"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = guild_id(ctx)?;
    let days = days.unwrap_or(1).clamp(1, 7);

    guild
        .ban(ctx.http(), target, u32::from(days), reason.as_deref())
        .await?;
    let config = store::config(&ctx.data().cache, &ctx.data().db, guild.get()).await;
    let case = actions::record_case(
        &ctx.data().db,
        guild.get(),
        target,
        ctx.author().id.get(),
        CaseAction::Softban,
        reason.as_deref(),
        None,
    )
    .await;

    if let Err(error) = guild.unban(ctx.http(), target, reason.as_deref()).await {
        tracing::error!(%error, user_id = target.get(), "softban unban failed; member remains banned");
        return send_card(
            ctx,
            Card::lines(
                "⚠️ Softban Partially Applied",
                vec![
                    format!(
                        "<@{}> was banned, but the automatic unban failed.",
                        target.get()
                    ),
                    "The member is still banned. Unban them manually.".to_string(),
                ],
            )
            .kind(CardKind::Warning),
        )
        .await;
    }

    actions::notify(
        ctx.http(),
        &config,
        target,
        CaseAction::Softban,
        case.as_ref(),
        reason.as_deref(),
    )
    .await;
    ctx.send(
        action_success_card(&CaseAction::Softban, target.get(), case, reason.as_deref()).reply(),
    )
    .await?;
    Ok(())
}

/// Quarantine or release a member.
#[poise::command(
    slash_command,
    prefix_command,
    guild_only,
    default_member_permissions = "MODERATE_MEMBERS",
    required_bot_permissions = "MANAGE_ROLES",
    subcommands("quarantine_add", "quarantine_remove")
)]
pub async fn quarantine(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    send_card(
        ctx,
        Card::lines(
            "🚧 Quarantine",
            vec!["Use `/quarantine add` or `/quarantine remove`.".to_string()],
        )
        .kind(CardKind::Info),
    )
    .await
}

async fn quarantine_role_or_reply(
    ctx: Context<'_>,
    config: &ModConfig,
) -> Result<Option<u64>, Error> {
    if let Some(role) = config.quarantine_role {
        return Ok(Some(role));
    }
    send_card(
        ctx,
        Card::lines(
            "⚠️ Quarantine Not Configured",
            vec!["No quarantine role is configured for this server.".to_string()],
        )
        .kind(CardKind::Warning),
    )
    .await?;
    Ok(None)
}

/// Apply the configured quarantine role to a member.
#[poise::command(
    slash_command,
    prefix_command,
    rename = "add",
    guild_only,
    default_member_permissions = "MODERATE_MEMBERS",
    required_bot_permissions = "MANAGE_ROLES"
)]
pub async fn quarantine_add(
    ctx: Context<'_>,
    #[description = "Member to quarantine"] member: UserId,
    #[description = "Reason for quarantine"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = guild_id(ctx)?;
    let config = store::config(&ctx.data().cache, &ctx.data().db, guild.get()).await;
    let Some(role) = quarantine_role_or_reply(ctx, &config).await? else {
        return Ok(());
    };

    guild
        .add_member_role(ctx.http(), member, RoleId::new(role), reason.as_deref())
        .await?;

    let case = actions::record_case(
        &ctx.data().db,
        guild.get(),
        member,
        ctx.author().id.get(),
        CaseAction::Quarantine,
        reason.as_deref(),
        None,
    )
    .await;
    actions::notify(
        ctx.http(),
        &config,
        member,
        CaseAction::Quarantine,
        case.as_ref(),
        reason.as_deref(),
    )
    .await;

    ctx.send(
        action_success_card(
            &CaseAction::Quarantine,
            member.get(),
            case,
            reason.as_deref(),
        )
        .reply(),
    )
    .await?;
    Ok(())
}

/// Strip the configured quarantine role from a member.
#[poise::command(
    slash_command,
    prefix_command,
    rename = "remove",
    guild_only,
    default_member_permissions = "MODERATE_MEMBERS",
    required_bot_permissions = "MANAGE_ROLES"
)]
pub async fn quarantine_remove(
    ctx: Context<'_>,
    #[description = "Member to release"] member: UserId,
    #[description = "Reason for release"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = guild_id(ctx)?;
    let config = store::config(&ctx.data().cache, &ctx.data().db, guild.get()).await;
    let Some(role) = quarantine_role_or_reply(ctx, &config).await? else {
        return Ok(());
    };

    guild
        .remove_member_role(ctx.http(), member, RoleId::new(role), reason.as_deref())
        .await?;

    let case = actions::record_case(
        &ctx.data().db,
        guild.get(),
        member,
        ctx.author().id.get(),
        CaseAction::Unquarantine,
        reason.as_deref(),
        None,
    )
    .await;
    actions::notify(
        ctx.http(),
        &config,
        member,
        CaseAction::Unquarantine,
        case.as_ref(),
        reason.as_deref(),
    )
    .await;

    ctx.send(
        action_success_card(
            &CaseAction::Unquarantine,
            member.get(),
            case,
            reason.as_deref(),
        )
        .reply(),
    )
    .await?;
    Ok(())
}
