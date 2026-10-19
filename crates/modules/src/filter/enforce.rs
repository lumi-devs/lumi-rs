use std::sync::OnceLock;

use regex::Regex;

use super::store::FilterRules;

/// Constant invite-link pattern (mirrors lumi's `InviteRe`).
const INVITE_PATTERN: &str = r"(?i)(?:discord\.(?:gg|com/invite)|discordapp\.com/invite)/([\w-]+)";
/// Constant URL pattern (mirrors lumi's `UrlRe`).
const URL_PATTERN: &str = r#"(?i)https?://([^\s/<>"']+)"#;

/// Which rule a message tripped, with the matched detail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reason {
    Term(String),
    Regex(String),
    Invite(String),
    Link(String),
    Mentions(u64),
    Caps(i64),
}

impl Reason {
    /// Human-readable reason, used in the warning message.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Term(_) => "a filtered term",
            Self::Regex(_) => "a filtered pattern",
            Self::Invite(_) => "a server invite link",
            Self::Link(_) => "a link that is not allowed here",
            Self::Mentions(_) => "too many mentions",
            Self::Caps(_) => "excessive caps",
        }
    }

    /// Rule name, used in logs and the audit-log timeout reason.
    pub fn action(&self) -> &'static str {
        match self {
            Self::Term(_) => "term",
            Self::Regex(_) => "regex",
            Self::Invite(_) => "invite",
            Self::Link(_) => "link",
            Self::Mentions(_) => "mentions",
            Self::Caps(_) => "caps",
        }
    }

    pub fn detail(&self) -> String {
        match self {
            Self::Term(term) => format!("term `{term}`"),
            Self::Regex(pattern) => format!("regex `{pattern}`"),
            Self::Invite(code) => format!("invite `{code}`"),
            Self::Link(host) => format!("link `{host}`"),
            Self::Mentions(count) => format!("{count} mentions"),
            Self::Caps(percent) => format!("{percent}% caps"),
        }
    }
}

/// What the filter decided for one message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub delete: bool,
    pub reasons: Vec<Reason>,
    pub timeout_minutes: Option<i64>,
    pub warn: bool,
}

impl Verdict {
    fn clean() -> Self {
        Self {
            delete: false,
            reasons: Vec::new(),
            timeout_minutes: None,
            warn: false,
        }
    }
}

/// Evaluate the rules in lumi's precedence order; the first hit wins.
pub fn evaluate(rules: &FilterRules, content: &str, mention_count: u64) -> Verdict {
    let Some(reason) = first_hit(rules, content, mention_count) else {
        return Verdict::clean();
    };
    Verdict {
        delete: true,
        timeout_minutes: timeout_for(rules, &reason),
        warn: !rules.warn_message.trim().is_empty(),
        reasons: vec![reason],
    }
}

fn first_hit(rules: &FilterRules, content: &str, mention_count: u64) -> Option<Reason> {
    if let Some(term) = rules.term_hit(content) {
        return Some(Reason::Term(term));
    }

    if let Some(regex) = rules.regexes.iter().find(|regex| regex.is_match(content)) {
        return Some(Reason::Regex(regex.as_str().to_string()));
    }

    if rules.block_invites {
        if let Some(code) = blocked_invite(content, &rules.invite_allow) {
            return Some(Reason::Invite(code));
        }
    }

    if rules.block_links {
        if let Some(host) = blocked_link(content, &rules.link_allow) {
            return Some(Reason::Link(host));
        }
    }

    if rules.max_mentions > 0 && mention_count as i64 > rules.max_mentions {
        return Some(Reason::Mentions(mention_count));
    }

    if rules.caps_pct > 0 && content.chars().count() >= rules.caps_min_len {
        let percent = caps_percent(content);
        if percent > rules.caps_pct as f64 {
            return Some(Reason::Caps(percent.round() as i64));
        }
    }

    None
}

fn timeout_for(rules: &FilterRules, reason: &Reason) -> Option<i64> {
    let specific = match reason {
        Reason::Invite(_) => rules.invite_timeout_minutes,
        Reason::Link(_) => rules.link_timeout_minutes,
        Reason::Mentions(_) | Reason::Caps(_) => rules.spam_timeout_minutes,
        Reason::Term(_) | Reason::Regex(_) => 0,
    };
    let minutes = if specific > 0 {
        specific
    } else {
        rules.timeout_minutes
    };
    (minutes > 0).then_some(minutes)
}

/// The invite code of the first blocked invite, or `None` if absent/allowlisted.
fn blocked_invite(content: &str, allow: &[String]) -> Option<String> {
    let captures = invite_regex()?.captures(content)?;
    let code = captures.get(1)?.as_str();
    let lower = code.to_lowercase();
    if allow.iter().any(|allowed| allowed == &lower) {
        None
    } else {
        Some(code.to_string())
    }
}

/// The host of the first non-allowlisted URL, or `None` if none.
fn blocked_link(content: &str, allow: &[String]) -> Option<String> {
    for captures in url_regex()?.captures_iter(content) {
        let Some(host_match) = captures.get(1) else {
            continue;
        };
        let host = host_match
            .as_str()
            .split(':')
            .next()
            .unwrap_or_default()
            .to_lowercase();
        if host.is_empty() {
            continue;
        }
        let allowed = allow
            .iter()
            .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")));
        if !allowed {
            return Some(host);
        }
    }
    None
}

/// Share of cased letters that are uppercase, as a percentage.
fn caps_percent(content: &str) -> f64 {
    let mut letters = 0_u64;
    let mut upper = 0_u64;
    for ch in content.chars() {
        if ch.is_alphabetic() {
            letters += 1;
            if ch.is_uppercase() {
                upper += 1;
            }
        }
    }
    if letters == 0 {
        0.0
    } else {
        (upper as f64 / letters as f64) * 100.0
    }
}

fn invite_regex() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(INVITE_PATTERN).ok()).as_ref()
}

fn url_regex() -> Option<&'static Regex> {
    static RE: OnceLock<Option<Regex>> = OnceLock::new();
    RE.get_or_init(|| Regex::new(URL_PATTERN).ok()).as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::store::FilterRules;
    use serde_json::{Value, json};

    fn rules(value: Value) -> FilterRules {
        let map = value.as_object().cloned().unwrap_or_default();
        FilterRules::from_map(&map)
    }

    #[test]
    fn clean_message_has_no_verdict() {
        let verdict = evaluate(&rules(json!({})), "hello world", 0);
        assert!(!verdict.delete);
        assert!(verdict.reasons.is_empty());
    }

    #[test]
    fn term_match_is_case_insensitive() {
        let verdict = evaluate(&rules(json!({ "terms": ["badword"] })), "This has BadWord", 0);
        assert!(verdict.delete);
        assert_eq!(verdict.reasons, vec![Reason::Term("badword".to_string())]);
    }

    #[test]
    fn invite_allowlist_is_respected() {
        let msg = "join https://discord.gg/abc123";
        let blocked = evaluate(&rules(json!({ "block_invites": true })), msg, 0);
        assert_eq!(blocked.reasons, vec![Reason::Invite("abc123".to_string())]);

        let allowed = evaluate(
            &rules(json!({ "block_invites": true, "invite_allowlist": ["abc123"] })),
            msg,
            0,
        );
        assert!(!allowed.delete);
    }

    #[test]
    fn link_allowlist_matches_subdomains() {
        let inner = rules(json!({ "block_links": true, "link_allowlist": ["github.com"] }));
        assert!(!evaluate(&inner, "see https://gist.github.com/x", 0).delete);
        assert!(evaluate(&inner, "see https://evil.example.com/x", 0).delete);
    }

    #[test]
    fn mention_limit_and_caps_rules() {
        let mentions = rules(json!({ "max_mentions": 3 }));
        assert!(!evaluate(&mentions, "hi", 3).delete);
        assert!(evaluate(&mentions, "hi", 4).delete);

        let caps = rules(json!({ "max_caps_percent": 60, "caps_min_length": 5 }));
        assert!(evaluate(&caps, "SHOUTING NOW", 0).delete);
        assert!(!evaluate(&caps, "shouting now", 0).delete);
    }

    #[test]
    fn timeout_uses_specific_then_base_then_disabled() {
        let overrides = rules(json!({
            "max_mentions": 1,
            "spam_timeout_minutes": 2,
            "block_invites": true,
            "invite_timeout_minutes": 20
        }));
        assert_eq!(evaluate(&overrides, "hi", 5).timeout_minutes, Some(2));
        assert_eq!(evaluate(&overrides, "discord.gg/x", 0).timeout_minutes, Some(20));

        let base = rules(json!({ "terms": ["badword"], "timeout_minutes": 5 }));
        assert_eq!(evaluate(&base, "badword", 0).timeout_minutes, Some(5));

        let disabled = rules(json!({ "terms": ["badword"] }));
        assert_eq!(evaluate(&disabled, "badword", 0).timeout_minutes, None);
    }
}
