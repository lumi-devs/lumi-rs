use std::sync::{Arc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use poise::serenity_prelude as serenity;
use regex::Regex;
use serenity::all::{
    ChannelType, EditMember, GenericChannelId, GuildId, Http, Message, PermissionOverwrite,
    PermissionOverwriteType, Permissions, Timestamp, UserId,
};

use lumi_kernel::Services;
use lumi_ui::{Card, CardKind};

use super::store::FilterRules;

const URL_PATTERN: &str = r#"(?i)https?://[^\s/<>"']+"#;
const CUSTOM_EMOJI_PATTERN: &str = r"<a?:[a-zA-Z0-9_]+:[0-9]+>";

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
    let custom = RE
        .get_or_init(|| Regex::new(CUSTOM_EMOJI_PATTERN).ok())
        .as_ref()
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
    RE.get_or_init(|| Regex::new(URL_PATTERN).ok())
        .as_ref()
        .is_some_and(|re| re.is_match(content))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

pub async fn add_heat(
    services: &Services,
    guild_id: u64,
    user_id: u64,
    points: f64,
    decay_per_minute: f64,
) -> Result<f64, redis::RedisError> {
    let mut conn = services.cache.redis();
    let key = format!("lumi:filter:{guild_id}:heat:{user_id}");
    let now = now_millis();

    let (stored_h, stored_t): (Option<f64>, Option<u64>) = redis::pipe()
        .hget(&key, "h")
        .hget(&key, "t")
        .query_async(&mut conn)
        .await?;

    let h = stored_h.unwrap_or(0.0);
    let t = stored_t.unwrap_or(now);

    let decayed = if decay_per_minute > 0.0 {
        let minutes = (now.saturating_sub(t) as f64) / 60_000.0;
        (h - minutes * decay_per_minute).max(0.0)
    } else {
        h.max(0.0)
    };

    let nxt = decayed + points;
    let ttl_secs = if decay_per_minute > 0.0 {
        ((nxt / decay_per_minute) * 60.0).ceil() as u64 + 60
    } else {
        3600
    };

    let _: () = redis::pipe()
        .hset(&key, "h", nxt)
        .hset(&key, "t", now)
        .expire(&key, ttl_secs.max(60) as i64)
        .query_async(&mut conn)
        .await?;

    Ok(nxt)
}

pub async fn clear_heat(services: &Services, guild_id: u64, user_id: u64) {
    let mut conn = services.cache.redis();
    let key = format!("lumi:filter:{guild_id}:heat:{user_id}");
    let _: Result<(), _> = redis::cmd("DEL").arg(&key).query_async(&mut conn).await;
}

pub async fn claim_escalation(
    services: &Services,
    guild_id: u64,
    user_id: u64,
    action: &'static str,
) -> bool {
    let mut conn = services.cache.redis();
    let key = format!("lumi:filter:{guild_id}:heatacted:{user_id}:{action}");
    let res: Option<String> = redis::cmd("SET")
        .arg(&key)
        .arg("1")
        .arg("EX")
        .arg(30)
        .arg("NX")
        .query_async(&mut conn)
        .await
        .ok();
    res.as_deref() == Some("OK")
}

pub async fn record_mentions(
    services: &Services,
    guild_id: u64,
    count: u64,
    window_seconds: u64,
) -> Result<u64, redis::RedisError> {
    let mut conn = services.cache.redis();
    let key = format!("lumi:filter:{guild_id}:mentions");
    let total: u64 = redis::cmd("INCRBY")
        .arg(&key)
        .arg(count)
        .query_async(&mut conn)
        .await?;
    let _: Result<(), _> = redis::cmd("EXPIRE")
        .arg(&key)
        .arg(if window_seconds > 0 { window_seconds } else { 10 })
        .arg("NX")
        .query_async(&mut conn)
        .await;
    Ok(total)
}

pub async fn activate_auto_lockdown(
    services: &Services,
    guild_id: u64,
    duration_minutes: u64,
) -> bool {
    let mut conn = services.cache.redis();
    let key = format!("lumi:filter:{guild_id}:autolockdown");
    let res: Option<String> = redis::cmd("SET")
        .arg(&key)
        .arg(now_millis().to_string())
        .arg("EX")
        .arg((duration_minutes * 60).max(60))
        .arg("NX")
        .query_async(&mut conn)
        .await
        .ok();
    res.as_deref() == Some("OK")
}

pub async fn lock_text_channels(ctx: &serenity::Context, guild_id: GuildId) -> u32 {
    let everyone = guild_id.everyone_role();
    let Ok(channels) = guild_id.channels(&ctx.http).await else {
        return 0;
    };

    let mut modified = 0u32;
    for ch in channels.iter() {
        if !matches!(ch.base.kind, ChannelType::Text | ChannelType::News) {
            continue;
        }
        let overwrite = PermissionOverwrite {
            allow: Permissions::empty(),
            deny: Permissions::SEND_MESSAGES,
            kind: PermissionOverwriteType::Role(everyone),
        };
        if ch
            .id
            .create_permission(&ctx.http, overwrite, Some("Filter auto-lockdown"))
            .await
            .is_ok()
        {
            modified += 1;
        }
    }
    modified
}

pub async fn unlock_text_channels(http: &Arc<Http>, guild_id: GuildId) -> u32 {
    let everyone = guild_id.everyone_role();
    let Ok(channels) = guild_id.channels(http).await else {
        return 0;
    };

    let mut modified = 0u32;
    for ch in channels.iter() {
        if !matches!(ch.base.kind, ChannelType::Text | ChannelType::News) {
            continue;
        }
        if ch
            .id
            .delete_permission(
                http,
                PermissionOverwriteType::Role(everyone),
                Some("Filter auto-lockdown lifted"),
            )
            .await
            .is_ok()
        {
            modified += 1;
        }
    }
    modified
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

    let modified = lock_text_channels(ctx, guild_id).await;
    let detail = format!(
        "Mention flood: {total} mentions within {}s. Locked {modified} channel(s) for {duration_minutes}m.",
        rules.lockdown_window_seconds
    );
    tracing::warn!(guild_id = guild_id.get(), %detail, "filter: auto-lockdown triggered");

    if let Some(log_channel) = rules.log_channel {
        let card = Card::lines("🚨 Auto-Lockdown Triggered", vec![detail.clone()])
            .kind(CardKind::Error);
        let _ = GenericChannelId::new(log_channel)
            .send_message(&ctx.http, card.into_reply().message())
            .await;
    }

    // Schedule background unlock task
    let http = Arc::clone(&ctx.http);
    let cache = Arc::clone(&services.cache);
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(duration_minutes * 60)).await;
        unlock_text_channels(&http, guild_id).await;
        let mut conn = cache.redis();
        let key = format!("lumi:filter:{guild_id}:autolockdown");
        let _: Result<(), _> = redis::cmd("DEL").arg(&key).query_async(&mut conn).await;
        tracing::info!(guild_id = guild_id.get(), "filter: auto-lockdown expired and lifted");
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
        points += (rules.heat_per_mention * mention_count as i64) as f64;
    }
    if was_filter_hit && rules.heat_per_filter_hit > 0 {
        points += rules.heat_per_filter_hit as f64;
    }
    if rules.heat_per_attachment > 0 && !message.attachments.is_empty() {
        points += (rules.heat_per_attachment * message.attachments.len() as i64) as f64;
    }
    if rules.heat_per_emoji > 0 {
        let emojis = count_emoji(&message.content);
        if emojis > 0 {
            points += (rules.heat_per_emoji * emojis as i64) as f64;
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
            let reason = format!("Slow down: your messages are triggering spam thresholds ({heat:.1} heat).");
            let card = Card::lines("⚠️ Activity Warning", vec![reason.clone()])
                .kind(CardKind::Warning);
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
                Timestamp::now().unix_timestamp() + duration_mins * 60,
            );
            if let Ok(until) = until {
                let reason = format!("Heat escalation: exceeded {} heat ({heat:.1})", rules.heat_timeout);
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
            let reason = format!("Heat escalation: exceeded {} heat ({heat:.1})", rules.heat_quarantine);
            if let Ok(Some(quarantine_role)) =
                crate::moderation::store::quarantine_role(&services.db, guild_id.get()).await
            {
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
        }
        HeatAction::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        assert!(is_zalgo("z\u{0300}\u{0301}\u{0302}\u{0303}\u{0304}a\u{0305}\u{0306}l\u{0307}\u{0308}g\u{0309}o"));
    }
}
