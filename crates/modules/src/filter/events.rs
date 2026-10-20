use poise::serenity_prelude as serenity;
use serenity::all::{
    EditMember, GenericChannelId, Message, MessageUpdateEvent, Permissions, Timestamp,
};

use lumi_kernel::Services;
use lumi_ui::{Card, CardKind};

use super::enforce;
use super::heat;
use super::store::{self, FilterRules};
use crate::text::cut;

/// Screen a created message. Returns `true` when it was enforced (deleted), so
/// the caller can stop before running the other message listeners.
pub async fn on_message(ctx: &serenity::Context, services: &Services, message: &Message) -> bool {
    screen(ctx, services, message).await
}

/// Screen an edited message; same return contract as [`on_message`].
pub async fn on_message_update(
    ctx: &serenity::Context,
    services: &Services,
    event: &MessageUpdateEvent,
) -> bool {
    screen(ctx, services, &event.message).await
}

async fn screen(ctx: &serenity::Context, services: &Services, message: &Message) -> bool {
    let Some(guild_id) = message.guild_id else {
        return false;
    };
    if message.author.bot() {
        return false;
    }
    let rules = store::rules_for(&services.cache, &services.db, guild_id.get()).await;
    if !rules.active() {
        return false;
    }
    if is_exempt(ctx, message, &rules) {
        return false;
    }

    let mention_count = (message.mentions.len() + message.mention_roles.len()) as u64;
    heat::check_mention_flood(ctx, services, message, mention_count, &rules).await;

    let verdict = enforce::evaluate(&rules, message.content.as_str(), mention_count);
    let hit = verdict.delete;
    if hit {
        apply(ctx, message, &rules, &verdict).await;
    }

    if rules.heat_enabled {
        heat::check_heat(
            ctx,
            services,
            message,
            message.author.id,
            mention_count,
            hit,
            &rules,
        )
        .await;
    }

    hit
}

/// Manage Messages holders and configured exempt roles bypass all rules.
fn is_exempt(ctx: &serenity::Context, message: &Message, rules: &FilterRules) -> bool {
    if let Some(member) = message.member.as_deref() {
        if member
            .roles
            .iter()
            .any(|role| rules.exempt_roles.contains(&role.get()))
        {
            return true;
        }
    }
    let Some(guild_id) = message.guild_id else {
        return false;
    };
    ctx.cache.guild(guild_id).is_some_and(|guild| {
        guild.members.get(&message.author.id).is_some_and(|member| {
            guild
                .member_permissions(member)
                .contains(Permissions::MANAGE_MESSAGES)
        })
    })
}

async fn apply(
    ctx: &serenity::Context,
    message: &Message,
    rules: &FilterRules,
    verdict: &enforce::Verdict,
) {
    let reason = verdict
        .reasons
        .first()
        .map_or_else(|| "a rule".to_string(), |reason| reason.detail());
    let action = verdict
        .reasons
        .first()
        .map_or("unknown", |reason| reason.action());

    if let Err(error) = message
        .channel_id
        .delete_message(&ctx.http, message.id, Some("Filter: matched a rule"))
        .await
    {
        tracing::warn!(%error, message_id = %message.id, "filter: failed to delete message");
    }

    if verdict.warn {
        warn(ctx, message, rules, verdict).await;
    }
    if let Some(minutes) = verdict.timeout_minutes {
        timeout(ctx, message, minutes).await;
    }
    log_hit(ctx, message, rules, action, &reason).await;
}

/// Transient in-channel V2 warning, deleted after a short delay (mirrors lumi).
async fn warn(
    ctx: &serenity::Context,
    message: &Message,
    rules: &FilterRules,
    verdict: &enforce::Verdict,
) {
    let label = verdict
        .reasons
        .first()
        .map_or("a rule", |reason| reason.label());
    let text = rules
        .warn_message
        .replace("{user}", &format!("<@{}>", message.author.id.get()))
        .replace("{reason}", label)
        .trim()
        .to_string();
    if text.is_empty() {
        return;
    }

    let card = Card::warning("Message Removed", text).into_reply();
    match message
        .channel_id
        .send_message(&ctx.http, card.message())
        .await
    {
        Ok(warning) => {
            let http = ctx.http.clone();
            let channel = message.channel_id;
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                if let Err(error) = channel
                    .delete_message(&http, warning.id, Some("Filter: warning"))
                    .await
                {
                    tracing::debug!(%error, "filter: failed to remove warning");
                }
            });
        }
        Err(error) => tracing::warn!(%error, "filter: failed to send warning"),
    }
}

async fn timeout(ctx: &serenity::Context, message: &Message, minutes: i64) {
    let Some(guild_id) = message.guild_id else {
        return;
    };
    let until = Timestamp::from_unix_timestamp(
        Timestamp::now()
            .unix_timestamp()
            .saturating_add(minutes.saturating_mul(60)),
    );
    let Ok(until) = until else {
        return;
    };
    let reason = format!("[Filter] Message matched a rule ({minutes}m timeout)");
    if let Err(error) = guild_id
        .edit_member(
            &ctx.http,
            message.author.id,
            EditMember::new()
                .disable_communication_until(until)
                .audit_log_reason(reason),
        )
        .await
    {
        tracing::warn!(%error, user_id = %message.author.id, "filter: failed to timeout member");
    }
}

async fn log_hit(
    ctx: &serenity::Context,
    message: &Message,
    rules: &FilterRules,
    action: &str,
    reason: &str,
) {
    let Some(channel) = rules.log_channel else {
        return;
    };
    let lines = vec![
        format!(
            "**Author**: <@{}> (`{}`)",
            message.author.id.get(),
            message.author.id.get()
        ),
        format!("**Rule**: `{action}`"),
        format!("**Reason**: {reason}"),
        format!("**Channel**: <#{}>", message.channel_id.get()),
        format!("**Message**: {}", cut(message.content.as_str(), 200)),
    ];
    let card = Card::lines("🚫 Filter", lines)
        .kind(CardKind::Error)
        .into_reply();
    if let Err(error) = GenericChannelId::new(channel)
        .send_message(&ctx.http, card.message())
        .await
    {
        tracing::warn!(%error, "filter: failed to log hit");
    }
}
