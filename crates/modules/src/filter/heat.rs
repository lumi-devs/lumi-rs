use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use poise::serenity_prelude as serenity;
use regex::Regex;
use serenity::all::{EditMember, GenericChannelId, Message, Timestamp, UserId};

use lumi_kernel::Services;
use lumi_ui::{Card, CardKind};

use super::store::FilterRules;
use crate::patterns::{CUSTOM_EMOJI_PATTERN, URL_PATTERN};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeatAction {
    None,
    Warn,
    Timeout,
    Quarantine,
}

pub fn heat_action(heat: f64, rules: &FilterRules) -> HeatAction {
    if rules.heat_quarantine > 0 && heat >= rules.heat_quarantine as f64 {
        HeatAction::Quarantine
    } else if rules.heat_timeout > 0 && heat >= rules.heat_timeout as f64 {
        HeatAction::Timeout
    } else if rules.heat_warn > 0 && heat >= rules.heat_warn as f64 {
        HeatAction::Warn
    } else {
        HeatAction::None
    }
}

pub fn is_zalgo(content: &str) -> bool {
    if content.is_empty() {
        return false;
    }
    let marks = content
        .chars()
        .filter(|&c| unicode_normalization::char::is_combining_mark(c))
        .count();
    (marks as f64 / content.chars().count() as f64) > 0.5
}

pub fn count_emoji(content: &str) -> u64 {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    let custom = crate::regex::cached(&RE, CUSTOM_EMOJI_PATTERN)
        .map_or(0, |re| re.find_iter(content).count() as u64);
    let unicode = content
        .chars()
        .filter(|&c| {
            matches!(c,
                '\u{1F300}'..='\u{1F5FF}'
                | '\u{1F600}'..='\u{1F64F}'
                | '\u{1F680}'..='\u{1F6FF}'
                | '\u{2600}'..='\u{26FF}'
                | '\u{2700}'..='\u{27BF}'
                | '\u{1F900}'..='\u{1F9FF}'
                | '\u{1FA70}'..='\u{1FAFF}'
            )
        })
        .count() as u64;
    custom + unicode
}

pub fn contains_link(content: &str) -> bool {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    crate::regex::cached(&RE, URL_PATTERN).is_some_and(|re| re.is_match(content))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub async fn add_heat(
    _services: &Services,
    guild_id: u64,
    user_id: u64,
    points: f64,
    decay_per_minute: f64,
) -> Result<f64, redis::RedisError> {
    Ok(super::state::add_heat(
        guild_id,
        user_id,
        points,
        decay_per_minute,
    ))
}

pub async fn clear_heat(_services: &Services, guild_id: u64, user_id: u64) {
    super::state::reset_heat(guild_id, user_id);
}

pub async fn claim_escalation(
    services: &Services,
    guild_id: u64,
    user_id: u64,
    action: &'static str,
) -> bool {
    let key = format!("filter:escalation:{guild_id}:{user_id}:{action}");
    services
        .cache
        .claim_local(&key, std::time::Duration::from_secs(30))
}

pub async fn record_mentions(
    _services: &Services,
    guild_id: u64,
    count: u64,
    window_seconds: u64,
) -> Result<u64, redis::RedisError> {
    Ok(super::state::record_mentions(
        guild_id,
        count,
        window_seconds,
    ))
}

pub async fn activate_auto_lockdown(
    services: &Services,
    guild_id: u64,
    duration_minutes: u64,
) -> bool {
    let mut conn = services.cache.redis();
    let key = format!("lumi:filter:{guild_id}:autolockdown");
    // NX ensures only one shard schedules the unlock for a lockdown.
    match redis::cmd("SET")
        .arg(&key)
        .arg(now_millis().to_string())
        .arg("EX")
        .arg(duration_minutes.saturating_mul(60).max(60))
        .arg("NX")
        .query_async::<Option<String>>(&mut conn)
        .await
    {
        Ok(res) => res.as_deref() == Some("OK"),
        Err(error) => {
            tracing::warn!(%error, guild_id, "filter: failed to set auto-lockdown key in redis");
            false
        }
    }
}

pub async fn check_mention_flood(
    ctx: &serenity::Context,
    services: &Services,
    message: &Message,
    mention_count: u64,
    rules: &FilterRules,
) {
    if mention_count == 0 || rules.lockdown_mention_threshold <= 0 {
        return;
    }
    let Some(guild_id) = message.guild_id else {
        return;
    };

    let total = match record_mentions(
        services,
        guild_id.get(),
        mention_count,
        rules.lockdown_window_seconds.max(1) as u64,
    )
    .await
    {
        Ok(t) => t,
        Err(err) => {
            tracing::warn!(%err, "filter: failed to record mentions");
            return;
        }
    };

    if total < rules.lockdown_mention_threshold as u64 {
        return;
    }

    let duration_minutes = rules.lockdown_duration_minutes.max(1) as u64;
    if !activate_auto_lockdown(services, guild_id.get(), duration_minutes).await {
        return;
    }

    let modified = crate::moderation::channels::set_guild_locked(
        &ctx.http,
        guild_id,
        true,
        "Filter auto-lockdown",
    )
    .await
    .modified;
    let detail = format!(
        "Mention flood: {total} mentions within {}s. Locked {modified} channel(s) for {duration_minutes}m.",
        rules.lockdown_window_seconds
    );
    tracing::warn!(guild_id = guild_id.get(), %detail, "filter: auto-lockdown triggered");

    if let Some(log_channel) = rules.log_channel {
        let card =
            Card::lines("🚨 Auto-Lockdown Triggered", vec![detail.clone()]).kind(CardKind::Error);
        let _ = GenericChannelId::new(log_channel)
            .send_message(&ctx.http, card.into_reply().message())
            .await;
    }

    let http = Arc::clone(&ctx.http);
    let cache = Arc::clone(&services.cache);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(duration_minutes.saturating_mul(60))).await;
        crate::moderation::channels::set_guild_locked(
            &http,
            guild_id,
            false,
            "Filter auto-lockdown lifted",
        )
        .await;
        let mut conn = cache.redis();
        let key = format!("lumi:filter:{guild_id}:autolockdown");
        if let Err(error) = redis::cmd("DEL").arg(&key).query_async::<()>(&mut conn).await {
            tracing::warn!(
                %error,
                guild_id = guild_id.get(),
                "filter: failed to delete auto-lockdown key from redis"
            );
        }
        tracing::info!(
            guild_id = guild_id.get(),
            "filter: auto-lockdown expired and lifted"
        );
    });
}

pub async fn check_heat(
    ctx: &serenity::Context,
    services: &Services,
    message: &Message,
    user_id: UserId,
    mention_count: u64,
    was_filter_hit: bool,
    rules: &FilterRules,
) {
    if !rules.heat_enabled {
        return;
    }
    let Some(guild_id) = message.guild_id else {
        return;
    };

    let mut points = rules.heat_per_message as f64;
    if rules.heat_per_mention > 0 && mention_count > 0 {
        let mentions = i64::try_from(mention_count).unwrap_or(i64::MAX);
        points += rules.heat_per_mention.saturating_mul(mentions) as f64;
    }
    if was_filter_hit && rules.heat_per_filter_hit > 0 {
        points += rules.heat_per_filter_hit as f64;
    }
    if rules.heat_per_attachment > 0 && !message.attachments.is_empty() {
        let count = i64::try_from(message.attachments.len()).unwrap_or(i64::MAX);
        points += rules.heat_per_attachment.saturating_mul(count) as f64;
    }
    if rules.heat_per_emoji > 0 {
        let emojis = count_emoji(&message.content);
        if emojis > 0 {
            let emoji_count = i64::try_from(emojis).unwrap_or(i64::MAX);
            points += rules.heat_per_emoji.saturating_mul(emoji_count) as f64;
        }
    }
    if rules.heat_per_link > 0 && contains_link(&message.content) {
        points += rules.heat_per_link as f64;
    }
    if rules.heat_per_zalgo > 0 && is_zalgo(&message.content) {
        points += rules.heat_per_zalgo as f64;
    }

    if points <= 0.0 {
        return;
    }

    let heat = match add_heat(
        services,
        guild_id.get(),
        user_id.get(),
        points,
        rules.heat_decay_per_minute as f64,
    )
    .await
    {
        Ok(h) => h,
        Err(err) => {
            tracing::warn!(%err, "filter: failed to add heat");
            return;
        }
    };

    let action = heat_action(heat, rules);
    if action == HeatAction::None {
        return;
    }

    let action_str = match action {
        HeatAction::Warn => "warn",
        HeatAction::Timeout => "timeout",
        HeatAction::Quarantine => "quarantine",
        HeatAction::None => return,
    };

    if !claim_escalation(services, guild_id.get(), user_id.get(), action_str).await {
        return;
    }

    match action {
        HeatAction::Warn => {
            let reason = format!(
                "Slow down: your messages are triggering spam thresholds ({heat:.1} heat)."
            );
            let card =
                Card::lines("⚠️ Activity Warning", vec![reason.clone()]).kind(CardKind::Warning);
            let _ = message
                .channel_id
                .send_message(&ctx.http, card.into_reply().message())
                .await;
            if let Some(log_channel) = rules.log_channel {
                let log_card = Card::lines(
                    "⚠️ Heat Warning Issued",
                    vec![
                        format!("**Target**: <@{}>", user_id.get()),
                        format!("**Channel**: <#{}>", message.channel_id.get()),
                        format!("**Heat Level**: {heat:.1}"),
                    ],
                )
                .kind(CardKind::Warning);
                let _ = GenericChannelId::new(log_channel)
                    .send_message(&ctx.http, log_card.into_reply().message())
                    .await;
            }
        }
        HeatAction::Timeout => {
            let duration_mins = rules.heat_timeout_minutes.max(1);
            let until = Timestamp::from_unix_timestamp(
                Timestamp::now()
                    .unix_timestamp()
                    .saturating_add(duration_mins.saturating_mul(60)),
            );
            if let Ok(until) = until {
                let reason = format!(
                    "Heat escalation: exceeded {} heat ({heat:.1})",
                    rules.heat_timeout
                );
                if let Err(err) = guild_id
                    .edit_member(
                        &ctx.http,
                        user_id,
                        EditMember::new()
                            .disable_communication_until(until)
                            .audit_log_reason(&reason),
                    )
                    .await
                {
                    tracing::warn!(%err, "filter: heat timeout failed");
                } else if let Some(log_channel) = rules.log_channel {
                    let log_card = Card::lines(
                        "⏳ Heat Timeout Applied",
                        vec![
                            format!("**Target**: <@{}>", user_id.get()),
                            format!("**Duration**: {duration_mins} minute(s)"),
                            format!("**Heat Level**: {heat:.1}"),
                            format!("**Reason**: {reason}"),
                        ],
                    )
                    .kind(CardKind::Warning);
                    let _ = GenericChannelId::new(log_channel)
                        .send_message(&ctx.http, log_card.into_reply().message())
                        .await;
                }
            }
        }
        HeatAction::Quarantine => {
            clear_heat(services, guild_id.get(), user_id.get()).await;
            let reason = format!(
                "Heat escalation: exceeded {} heat ({heat:.1})",
                rules.heat_quarantine
            );
            match crate::moderation::store::quarantine_role(
                &services.cache,
                &services.db,
                guild_id.get(),
            )
            .await
            {
                Ok(Some(quarantine_role)) => {
                    let _ = guild_id
                        .edit_member(
                            &ctx.http,
                            user_id,
                            EditMember::new().roles(vec![serenity::all::RoleId::new(quarantine_role)]),
                        )
                        .await;
                    if let Some(log_channel) = rules.log_channel {
                        let log_card = Card::lines(
                            "☣️ Member Quarantined (Heat)",
                            vec![
                                format!("**Target**: <@{}>", user_id.get()),
                                format!("**Heat Level**: {heat:.1}"),
                                format!("**Reason**: {reason}"),
                            ],
                        )
                        .kind(CardKind::Error);
                        let _ = GenericChannelId::new(log_channel)
                            .send_message(&ctx.http, log_card.into_reply().message())
                            .await;
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    tracing::warn!(
                        %error,
                        guild_id = guild_id.get(),
                        user_id = user_id.get(),
                        "filter: failed to look up quarantine role"
                    );
                }
            }
        }
        HeatAction::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_rules() -> FilterRules {
        let map = serde_json::json!({
            "heat_enabled": true,
            "heat_warn": 10,
            "heat_timeout": 20,
            "heat_quarantine": 30
        });
        let obj = map.as_object().cloned().unwrap_or_default();
        FilterRules::from_map(&obj)
    }

    #[test]
    fn test_heat_actions() {
        let rules = test_rules();
        assert_eq!(heat_action(5.0, &rules), HeatAction::None);
        assert_eq!(heat_action(10.0, &rules), HeatAction::Warn);
        assert_eq!(heat_action(15.0, &rules), HeatAction::Warn);
        assert_eq!(heat_action(20.0, &rules), HeatAction::Timeout);
        assert_eq!(heat_action(35.0, &rules), HeatAction::Quarantine);
    }

    #[test]
    fn test_emoji_and_links() {
        assert!(contains_link("Check https://example.com/foo"));
        assert!(!contains_link("Just normal text"));

        assert_eq!(count_emoji("Hello 👋 world 🚀"), 2);
        assert_eq!(count_emoji("Custom <:kek:123456789> emoji"), 1);
    }

    #[test]
    fn test_zalgo_detection() {
        assert!(!is_zalgo("Clean normal string"));
        assert!(is_zalgo(
            "z\u{0300}\u{0301}\u{0302}\u{0303}\u{0304}a\u{0305}\u{0306}l\u{0307}\u{0308}g\u{0309}o"
        ));
    }
}
