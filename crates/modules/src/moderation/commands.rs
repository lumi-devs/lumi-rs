use poise::serenity_prelude as serenity;
use serenity::all::{
    ChannelId, ChannelType, CreateAllowedMentions, CreateMessage, EditMember, GenericChannelId,
    PermissionOverwrite, PermissionOverwriteType, Permissions, RoleId, UserId,
};

use lumi_db::entities::enums::CaseAction;
use lumi_kernel::bot::{Command, Context, Error};
use lumi_ui::{Card, CardKind};

use super::store::{self, CreateCaseParams};
use super::views::{case_card, mod_action_success_card, notes_list_card};

async fn autocomplete_reason<'a>(
    _ctx: Context<'a>,
    partial: &'a str,
) -> serenity::CreateAutocompleteResponse<'a> {
    const PRESETS: &[&str] = &[
        "⚠️ Ban evasion",
        "🤖 Compromised account",
        "🚫 NSFW content",
        "🎯 Raiding",
        "🔗 Scam links",
        "⛔ Self-botting",
        "📧 Spam",
        "💬 Toxicity/Harassment",
    ];
    let choices: Vec<serenity::AutocompleteChoice> = PRESETS
        .iter()
        .filter(|r| r.to_lowercase().contains(&partial.to_lowercase()))
        .map(|r| serenity::AutocompleteChoice::new(*r, *r))
        .collect();

    serenity::CreateAutocompleteResponse::new().set_choices(choices)
}

/// Warn a guild member.
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "MODERATE_MEMBERS")]
pub async fn warn(
    ctx: Context<'_>,
    #[description = "The target user"] user: UserId,
    #[description = "Reason for warning"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
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
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "KICK_MEMBERS")]
pub async fn kick(
    ctx: Context<'_>,
    #[description = "The target user"] user: UserId,
    #[description = "Reason for kick"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
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
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "BAN_MEMBERS")]
pub async fn ban(
    ctx: Context<'_>,
    #[description = "The target user"] user: UserId,
    #[description = "Reason for ban"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
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
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "BAN_MEMBERS")]
pub async fn unban(
    ctx: Context<'_>,
    #[description = "The target user id"] user: UserId,
    #[description = "Reason for unban"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
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
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "MODERATE_MEMBERS")]
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
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "MODERATE_MEMBERS")]
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
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "MODERATE_MEMBERS")]
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

fn parse_duration(input: &str) -> Option<std::time::Duration> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }
    let (num_part, unit_part) = input.split_at(input.find(|c: char| !c.is_ascii_digit()).unwrap_or(input.len()));
    let num: u64 = num_part.parse().ok()?;
    let secs = match unit_part.to_lowercase().as_str() {
        "s" | "sec" | "secs" => num,
        "m" | "min" | "mins" => num * 60,
        "h" | "hr" | "hrs" | "hour" | "hours" => num * 3600,
        "d" | "day" | "days" => num * 86400,
        "w" | "week" | "weeks" => num * 604800,
        _ => return None,
    };
    Some(std::time::Duration::from_secs(secs))
}

/// Timeout / mute a member in the server.
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "MODERATE_MEMBERS")]
pub async fn mute(
    ctx: Context<'_>,
    #[description = "The target user"] user: UserId,
    #[description = "Duration e.g. 10m, 1h, 1d (max 28d)"] duration: String,
    #[description = "Reason for mute"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let mod_id = ctx.author().id.get();

    let dur = parse_duration(&duration).ok_or_else(|| {
        anyhow::anyhow!("Invalid duration format. Use formats like `10m`, `2h`, `1d`.")
    })?;

    let dur_secs = dur.as_secs().min(28 * 86400); // Discord max timeout is 28 days
    let until = serenity::Timestamp::from_unix_timestamp(
        chrono::Utc::now().timestamp() + dur_secs as i64,
    )?;

    // Apply timeout to guild member
    let mut member = guild.member(ctx.http(), user).await?;
    member.disable_communication_until(ctx.http(), until).await?;

    let case = store::create_case(
        &ctx.data().db,
        CreateCaseParams {
            guild_id: guild.get(),
            user_id: user.get(),
            moderator_id: mod_id,
            action: CaseAction::Mute,
            reason: reason.as_deref(),
            duration_seconds: Some(dur_secs as i32),
        },
    )
    .await?;

    let card = mod_action_success_card(&CaseAction::Mute, user.get(), case.case_number, reason.as_deref());
    ctx.send(card.reply()).await?;
    Ok(())
}

/// Remove timeout / unmute a member in the server.
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "MODERATE_MEMBERS")]
pub async fn unmute(
    ctx: Context<'_>,
    #[description = "The target user"] user: UserId,
    #[description = "Reason for unmute"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let mod_id = ctx.author().id.get();

    let mut member = guild.member(ctx.http(), user).await?;
    member.enable_communication(ctx.http()).await?;

    let case = store::create_case(
        &ctx.data().db,
        CreateCaseParams {
            guild_id: guild.get(),
            user_id: user.get(),
            moderator_id: mod_id,
            action: CaseAction::Unmute,
            reason: reason.as_deref(),
            duration_seconds: None,
        },
    )
    .await?;

    let card = mod_action_success_card(&CaseAction::Unmute, user.get(), case.case_number, reason.as_deref());
    ctx.send(card.reply()).await?;
    Ok(())
}

/// Softban a member (ban then immediately unban to purge recent messages).
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "BAN_MEMBERS")]
pub async fn softban(
    ctx: Context<'_>,
    #[description = "The target user"] target: UserId,
    #[description = "Days of messages to delete (1-7)"] #[min = 1] #[max = 7] days: Option<u8>,
    #[description = "Reason for softban"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let mod_id = ctx.author().id.get();
    let days = days.unwrap_or(1).clamp(1, 7);

    guild.ban(ctx.http(), target, u32::from(days), reason.as_deref()).await?;
    guild.unban(ctx.http(), target, reason.as_deref()).await?;

    let case = store::create_case(
        &ctx.data().db,
        CreateCaseParams {
            guild_id: guild.get(),
            user_id: target.get(),
            moderator_id: mod_id,
            action: CaseAction::Softban,
            reason: reason.as_deref(),
            duration_seconds: None,
        },
    )
    .await?;

    let card = mod_action_success_card(&CaseAction::Softban, target.get(), case.case_number, reason.as_deref());
    ctx.send(card.reply()).await?;
    Ok(())
}

/// Lock or unlock one channel for @everyone.
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "MANAGE_CHANNELS", subcommands("lock_enable", "lock_disable"))]
pub async fn lock(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let card = Card::lines("🔒 Lock", vec!["Use `/lock enable` or `/lock disable`.".to_string()])
        .kind(CardKind::Info);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Deny @everyone SendMessages in a channel.
#[poise::command(slash_command, prefix_command, rename = "enable")]
pub async fn lock_enable(
    ctx: Context<'_>,
    #[description = "Channel to lock (defaults to this one)"] channel: Option<GenericChannelId>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let channel_id = ChannelId::new(channel.unwrap_or_else(|| ctx.channel_id()).get());

    let overwrite = PermissionOverwrite {
        allow: Permissions::empty(),
        deny: Permissions::SEND_MESSAGES,
        kind: PermissionOverwriteType::Role(guild.everyone_role()),
    };
    channel_id.create_permission(ctx.http(), overwrite, Some("Channel locked")).await?;

    let card = Card::lines("🔒 Channel Locked", vec![format!("<#{}> is now locked - @everyone can no longer send messages.", channel_id.get())])
        .kind(CardKind::Success);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Restore @everyone SendMessages in a channel.
#[poise::command(slash_command, prefix_command, rename = "disable")]
pub async fn lock_disable(
    ctx: Context<'_>,
    #[description = "Channel to unlock (defaults to this one)"] channel: Option<GenericChannelId>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let channel_id = ChannelId::new(channel.unwrap_or_else(|| ctx.channel_id()).get());

    channel_id
        .delete_permission(ctx.http(), PermissionOverwriteType::Role(guild.everyone_role()), Some("Channel unlocked"))
        .await?;

    let card = Card::lines("🔓 Channel Unlocked", vec![format!("<#{}> is now unlocked - @everyone can send messages again.", channel_id.get())])
        .kind(CardKind::Success);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Enable or disable server-wide channel lockdown.
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "ADMINISTRATOR", subcommands("lockdown_enable", "lockdown_disable"))]
pub async fn lockdown(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let card = Card::lines("🔒 Lockdown", vec!["Use `/lockdown enable` or `/lockdown disable`.".to_string()])
        .kind(CardKind::Info);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Lock down every text channel for @everyone.
#[poise::command(slash_command, prefix_command, rename = "enable")]
pub async fn lockdown_enable(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let everyone = guild.everyone_role();
    let channels = guild.channels(ctx.http()).await?;

    let (mut modified, mut failed) = (0u32, 0u32);
    for ch in channels.iter() {
        if !matches!(ch.base.kind, ChannelType::Text | ChannelType::News) {
            continue;
        }
        let overwrite = PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::SEND_MESSAGES,
            kind: PermissionOverwriteType::Role(everyone),
        };
        if ch.id.create_permission(ctx.http(), overwrite, Some("Server lockdown")).await.is_ok() {
            modified += 1;
        } else {
            failed += 1;
        }
    }

    let card = if modified == 0 && failed > 0 {
        Card::lines("❌ Lockdown Failed", vec![format!("Could not modify permissions for {} channel(s).", failed)])
            .kind(CardKind::Error)
    } else {
        Card::lines("🔒 Lockdown Enabled", vec![format!("Disabled SendMessages for @everyone in {} text channel(s).", modified)])
            .kind(CardKind::Success)
    };
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Lift the server-wide channel lockdown.
#[poise::command(slash_command, prefix_command, rename = "disable")]
pub async fn lockdown_disable(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let everyone = guild.everyone_role();
    let channels = guild.channels(ctx.http()).await?;

    let (mut modified, mut failed) = (0u32, 0u32);
    for ch in channels.iter() {
        if !matches!(ch.base.kind, ChannelType::Text | ChannelType::News) {
            continue;
        }
        if ch
            .id
            .delete_permission(ctx.http(), PermissionOverwriteType::Role(everyone), Some("Server lockdown lifted"))
            .await
            .is_ok()
        {
            modified += 1;
        } else {
            failed += 1;
        }
    }

    let card = if modified == 0 && failed > 0 {
        Card::lines("❌ Lockdown Disable Failed", vec![format!("Could not modify permissions for {} channel(s).", failed)])
            .kind(CardKind::Error)
    } else {
        Card::lines("🔓 Lockdown Disabled", vec![format!("Restored SendMessages for @everyone in {} text channel(s).", modified)])
            .kind(CardKind::Success)
    };
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Quarantine or release a member.
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "MODERATE_MEMBERS", subcommands("quarantine_add", "quarantine_remove"))]
pub async fn quarantine(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let card = Card::lines("🚧 Quarantine", vec!["Use `/quarantine add` or `/quarantine remove`.".to_string()])
        .kind(CardKind::Info);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Apply the configured quarantine role to a member.
#[poise::command(slash_command, prefix_command, rename = "add")]
pub async fn quarantine_add(
    ctx: Context<'_>,
    #[description = "Member to quarantine"] member: UserId,
    #[description = "Reason for quarantine"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let mod_id = ctx.author().id.get();

    let Some(role_id) = store::quarantine_role(&ctx.data().db, guild.get()).await? else {
        let card = Card::lines("⚠️ Quarantine Not Configured", vec!["No quarantine role is configured for this server.".to_string()])
            .kind(CardKind::Warning);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    };

    guild.add_member_role(ctx.http(), member, RoleId::new(role_id), reason.as_deref()).await?;

    let case = store::create_case(
        &ctx.data().db,
        CreateCaseParams {
            guild_id: guild.get(),
            user_id: member.get(),
            moderator_id: mod_id,
            action: CaseAction::Quarantine,
            reason: reason.as_deref(),
            duration_seconds: None,
        },
    )
    .await?;

    let card = mod_action_success_card(&CaseAction::Quarantine, member.get(), case.case_number, reason.as_deref());
    ctx.send(card.reply()).await?;
    Ok(())
}

/// Strip the configured quarantine role from a member.
#[poise::command(slash_command, prefix_command, rename = "remove")]
pub async fn quarantine_remove(
    ctx: Context<'_>,
    #[description = "Member to release"] member: UserId,
    #[description = "Reason for release"]
    #[autocomplete = "autocomplete_reason"]
    reason: Option<String>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;
    let mod_id = ctx.author().id.get();

    let Some(role_id) = store::quarantine_role(&ctx.data().db, guild.get()).await? else {
        let card = Card::lines("⚠️ Quarantine Not Configured", vec!["No quarantine role is configured for this server.".to_string()])
            .kind(CardKind::Warning);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    };

    guild.remove_member_role(ctx.http(), member, RoleId::new(role_id), reason.as_deref()).await?;

    let case = store::create_case(
        &ctx.data().db,
        CreateCaseParams {
            guild_id: guild.get(),
            user_id: member.get(),
            moderator_id: mod_id,
            action: CaseAction::Unquarantine,
            reason: reason.as_deref(),
            duration_seconds: None,
        },
    )
    .await?;

    let card = mod_action_success_card(&CaseAction::Unquarantine, member.get(), case.case_number, reason.as_deref());
    ctx.send(card.reply()).await?;
    Ok(())
}

/// Relay a direct message through the bot to a user.
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "MODERATE_MEMBERS")]
pub async fn dm(
    ctx: Context<'_>,
    #[description = "User to message"] user: UserId,
    #[description = "Message to send"] #[max_length = 2000] message: String,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild_name = ctx.guild().map_or_else(|| "the server".to_string(), |g| g.name.to_string());

    let card = Card::lines(format!("📨 Message from {guild_name} staff"), vec![message]).kind(CardKind::Info);
    if user.dm(ctx.http(), card.into_reply().message()).await.is_err() {
        let card = Card::lines("❌ Could Not Send", vec![format!("<@{}> has DMs closed or has blocked the bot.", user.get())])
            .kind(CardKind::Error);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    let card = Card::lines("✅ Message Sent", vec![format!("Sent your message to <@{}>.", user.get())])
        .kind(CardKind::Success);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

/// Relay a message through the bot into a channel.
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "MANAGE_MESSAGES")]
pub async fn say(
    ctx: Context<'_>,
    #[description = "Message to send"] #[max_length = 2000] message: String,
    #[description = "Channel to send in (defaults to this one)"] channel: Option<GenericChannelId>,
) -> Result<(), Error> {
    ctx.defer().await?;
    let channel_id = ChannelId::new(channel.unwrap_or_else(|| ctx.channel_id()).get());

    channel_id
        .send_message(
            ctx.http(),
            CreateMessage::new()
                .content(message)
                .allowed_mentions(CreateAllowedMentions::new()),
        )
        .await?;

    let card = Card::lines("✅ Message Sent", vec![format!("Relayed your message to <#{}>.", channel_id.get())])
        .kind(CardKind::Success);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

fn strip_hoisting(name: &str) -> String {
    name.trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim()
        .to_string()
}

/// Remove hoisting characters from a member's nickname.
#[poise::command(slash_command, prefix_command, guild_only, default_member_permissions = "MANAGE_NICKNAMES")]
pub async fn sanitize(
    ctx: Context<'_>,
    #[description = "Member to sanitize"] member: UserId,
) -> Result<(), Error> {
    ctx.defer().await?;
    let guild = ctx.guild_id().ok_or_else(|| anyhow::anyhow!("guild only"))?;

    let guild_member = guild.member(ctx.http(), member).await?;
    let current = guild_member.nick.as_deref().unwrap_or(&guild_member.user.name).to_string();
    let sanitized = strip_hoisting(&current);

    if sanitized == current {
        let card = Card::lines("✅ Nothing to Sanitize", vec![format!("<@{}>'s nickname has no hoisting characters.", member.get())])
            .kind(CardKind::Info);
        ctx.send(card.into_reply().reply()).await?;
        return Ok(());
    }

    // An empty nickname resets it, discarding the hoisted name entirely.
    guild.edit_member(ctx.http(), member, EditMember::new().nickname(sanitized)).await?;

    let card = Card::lines("🧹 Sanitized", vec![format!("Removed hoisting characters from <@{}>.", member.get())])
        .kind(CardKind::Success);
    ctx.send(card.into_reply().reply()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::strip_hoisting;

    #[test]
    fn strips_hoisting_prefix() {
        assert_eq!(strip_hoisting("!John"), "John");
        assert_eq!(strip_hoisting("   ***Jane"), "Jane");
        assert_eq!(strip_hoisting("😀Jane"), "Jane");
        assert_eq!(strip_hoisting("!!! "), "");
        assert_eq!(strip_hoisting("123John"), "123John");
        assert_eq!(strip_hoisting("John"), "John");
    }
}

pub fn commands() -> Vec<Command> {
    vec![
        warn(), kick(), ban(), unban(), mute(), unmute(), case(), reason(), note(), softban(),
        lock(), lockdown(), quarantine(), dm(), say(), sanitize(),
    ]
}
