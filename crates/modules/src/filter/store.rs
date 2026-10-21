use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, OnceLock};

use aho_corasick::AhoCorasick;
use moka::future::Cache as MokaCache;
use regex::RegexBuilder;
use sea_orm::DatabaseConnection;
use serde_json::{Map, Value};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

use lumi_kernel::{Cache, module_state};

use crate::config;

const MODULE: &str = "filter";

/// Mirrors lumi's `DefaultCapsMinLength`.
pub const DEFAULT_CAPS_MIN_LENGTH: i64 = 12;
/// Mirrors lumi's `DefaultWarnMessage`.
pub const DEFAULT_WARN_MESSAGE: &str = "{user}, your message was removed for containing {reason}.";
/// Mirrors lumi's `MaxRegexLength`.
const MAX_REGEX_LENGTH: usize = 256;
/// Upper bound on cached guild rule sets; the key includes the config hash, so
/// stale entries simply age out.
const RULES_CACHE_CAPACITY: u64 = 10_000;

/// Fully resolved, compiled filter config for one guild.
#[derive(Debug)]
pub struct FilterRules {
    /// Original banned terms, for display and logs.
    pub terms: Vec<String>,
    /// Aho-Corasick matcher over the normalised terms (lumi uses the same).
    term_matcher: Option<AhoCorasick>,
    /// Original term per matcher pattern, so a hit maps back to its display form.
    term_labels: Vec<String>,
    pub regexes: Vec<regex::Regex>,
    pub invite_allow: Vec<String>,
    pub block_invites: bool,
    pub link_allow: Vec<String>,
    pub block_links: bool,
    pub max_mentions: i64,
    pub caps_pct: i64,
    pub caps_min_len: usize,
    pub exempt_roles: Vec<u64>,
    pub timeout_minutes: i64,
    pub spam_timeout_minutes: i64,
    pub invite_timeout_minutes: i64,
    pub link_timeout_minutes: i64,
    pub warn_message: String,
    pub log_channel: Option<u64>,
    pub heat_enabled: bool,
    pub heat_per_message: i64,
    pub heat_per_mention: i64,
    pub heat_per_filter_hit: i64,
    pub heat_per_attachment: i64,
    pub heat_per_emoji: i64,
    pub heat_per_link: i64,
    pub heat_per_zalgo: i64,
    pub heat_decay_per_minute: i64,
    pub heat_warn: i64,
    pub heat_timeout: i64,
    pub heat_timeout_minutes: i64,
    pub heat_quarantine: i64,
    pub lockdown_mention_threshold: i64,
    pub lockdown_window_seconds: i64,
    pub lockdown_duration_minutes: i64,
}

impl FilterRules {
    /// Whether any rule could possibly fire; an unconfigured guild is skipped.
    #[must_use]
    pub fn active(&self) -> bool {
        self.heat_enabled
            || self.lockdown_mention_threshold > 0
            || self.term_matcher.is_some()
            || !self.regexes.is_empty()
            || self.block_invites
            || self.block_links
            || self.max_mentions > 0
            || self.caps_pct > 0
    }

    /// The first banned term found in `content`, matched on normalised text so
    /// case, accents and zero-width obfuscation do not evade it.
    #[must_use]
    pub fn term_hit(&self, content: &str) -> Option<String> {
        let matcher = self.term_matcher.as_ref()?;
        let normalized = normalize(content);
        let found = matcher.find(normalized.as_str())?;
        self.term_labels.get(found.pattern().as_usize()).cloned()
    }

    pub(crate) fn from_map(map: &Map<String, Value>) -> Self {
        let mut terms = Vec::new();
        let mut patterns = Vec::new();
        let mut labels = Vec::new();
        for term in config::string_list(map, "terms") {
            let normalized = normalize(&term);
            if normalized.is_empty() {
                continue;
            }
            patterns.push(normalized);
            labels.push(term.clone());
            terms.push(term);
        }
        let term_matcher = if patterns.is_empty() {
            None
        } else {
            AhoCorasick::builder()
                .build(&patterns)
                .map_err(|error| tracing::warn!(%error, "filter: failed to build term matcher"))
                .ok()
        };

        let regexes = compile_regexes(&config::string_list(map, "regex_rules"));
        let warn_message = map
            .get("warn_message")
            .and_then(Value::as_str)
            .map_or_else(|| DEFAULT_WARN_MESSAGE.to_string(), |text| text.to_string());

        Self {
            terms,
            term_matcher,
            term_labels: labels,
            regexes,
            invite_allow: lowercase(config::string_list(map, "invite_allowlist")),
            block_invites: config::boolean(map, "block_invites", false),
            link_allow: lowercase(config::string_list(map, "link_allowlist")),
            block_links: config::boolean(map, "block_links", false),
            max_mentions: config::integer(map, "max_mentions", 0),
            caps_pct: config::integer(map, "max_caps_percent", 0),
            caps_min_len: config::integer(map, "caps_min_length", DEFAULT_CAPS_MIN_LENGTH).max(1)
                as usize,
            exempt_roles: config::id_list(map, "exempt_roles"),
            timeout_minutes: config::integer(map, "timeout_minutes", 0),
            spam_timeout_minutes: config::integer(map, "spam_timeout_minutes", 0),
            invite_timeout_minutes: config::integer(map, "invite_timeout_minutes", 0),
            link_timeout_minutes: config::integer(map, "link_timeout_minutes", 0),
            warn_message,
            log_channel: config::id(map, "log_channel_id"),
            heat_enabled: config::boolean(map, "heat_enabled", false),
            heat_per_message: config::integer(map, "heat_per_message", 1),
            heat_per_mention: config::integer(map, "heat_per_mention", 2),
            heat_per_filter_hit: config::integer(map, "heat_per_filter_hit", 5),
            heat_per_attachment: config::integer(map, "heat_per_attachment", 2),
            heat_per_emoji: config::integer(map, "heat_per_emoji", 1),
            heat_per_link: config::integer(map, "heat_per_link", 2),
            heat_per_zalgo: config::integer(map, "heat_per_zalgo", 3),
            heat_decay_per_minute: config::integer(map, "heat_decay_per_minute", 5),
            heat_warn: config::integer(map, "heat_warn", 15),
            heat_timeout: config::integer(map, "heat_timeout", 30),
            heat_timeout_minutes: config::integer(map, "heat_timeout_minutes", 10),
            heat_quarantine: config::integer(map, "heat_quarantine", 50),
            lockdown_mention_threshold: config::integer(map, "lockdown_mention_threshold", 0),
            lockdown_window_seconds: config::integer(map, "lockdown_window_seconds", 10),
            lockdown_duration_minutes: config::integer(map, "lockdown_duration_minutes", 10),
        }
    }
}

/// NFKD-normalise, drop invisible and combining characters, then lowercase.
/// Mirrors lumi's `normalizeForMatch` so obfuscated terms still match.
#[must_use]
pub fn normalize(input: &str) -> String {
    input
        .nfkd()
        .filter(|ch| !is_invisible(*ch) && !is_combining_mark(*ch))
        .collect::<String>()
        .to_lowercase()
}

fn is_invisible(ch: char) -> bool {
    matches!(
        ch,
        '\u{00AD}' | '\u{200B}'..='\u{200F}' | '\u{2060}'..='\u{2064}' | '\u{FEFF}'
    )
}

/// Load the guild's filter rules, reading config once per call and reusing the
/// compiled set while the stored config is unchanged (hash-keyed).
pub async fn rules_for(
    kernel_cache: &Cache,
    db: &DatabaseConnection,
    guild_id: u64,
) -> Arc<FilterRules> {
    let map = module_state::config_cached(kernel_cache, db, guild_id, MODULE).await;
    let key = format!("{guild_id}:{:x}", map_hash(&map));
    if let Some(rules) = cache().get(&key).await {
        return rules;
    }

    let rules = Arc::new(FilterRules::from_map(&map));
    cache().insert(key, Arc::clone(&rules)).await;
    rules
}
fn cache() -> &'static MokaCache<String, Arc<FilterRules>> {
    static CACHE: OnceLock<MokaCache<String, Arc<FilterRules>>> = OnceLock::new();
    CACHE.get_or_init(|| {
        MokaCache::builder()
            .max_capacity(RULES_CACHE_CAPACITY)
            .build()
    })
}

fn map_hash(map: &Map<String, Value>) -> u64 {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    let mut hasher = DefaultHasher::new();
    for key in keys {
        key.hash(&mut hasher);
        if let Some(value) = map.get(key) {
            value.to_string().hash(&mut hasher);
        }
    }
    hasher.finish()
}

fn compile_regexes(patterns: &[String]) -> Vec<regex::Regex> {
    patterns
        .iter()
        .filter_map(|pattern| {
            if pattern.len() > MAX_REGEX_LENGTH {
                tracing::warn!(%pattern, "filter: skipping regex longer than 256 chars");
                return None;
            }
            let regex = RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .map_err(|error| {
                    tracing::warn!(%pattern, %error, "filter: skipping invalid regex");
                    error
                })
                .ok()?;
            if regex.is_match("") {
                tracing::warn!(%pattern, "filter: skipping regex that matches everything");
                return None;
            }
            Some(regex)
        })
        .collect()
}

fn lowercase(items: Vec<String>) -> Vec<String> {
    items.into_iter().map(|item| item.to_lowercase()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn map(value: Value) -> Map<String, Value> {
        value.as_object().cloned().unwrap_or_default()
    }

    #[test]
    fn empty_config_uses_defaults_and_is_inactive() {
        let rules = FilterRules::from_map(&map(json!({})));
        assert!(!rules.active());
        assert_eq!(rules.caps_min_len, DEFAULT_CAPS_MIN_LENGTH as usize);
        assert_eq!(rules.warn_message, DEFAULT_WARN_MESSAGE);
        assert!(rules.regexes.is_empty());
    }

    #[test]
    fn compiles_valid_and_skips_invalid_regex() {
        let rules = FilterRules::from_map(&map(json!({
            "regex_rules": ["^foo$", "(unclosed"]
        })));
        assert_eq!(rules.regexes.len(), 1);
        assert!(rules.active());
    }

    #[test]
    fn skips_regex_that_would_match_every_message() {
        let rules = FilterRules::from_map(&map(json!({
            "regex_rules": [".*", "a*", "^", "a|", "\\bfoo\\b"]
        })));
        assert_eq!(rules.regexes.len(), 1);
        assert_eq!(rules.regexes[0].as_str(), "\\bfoo\\b");
    }

    #[test]
    fn terms_match_case_accents_and_invisible_chars() {
        let rules = FilterRules::from_map(&map(json!({ "terms": ["BadWord", "café", ""] })));
        assert_eq!(rules.terms, vec!["BadWord", "café"]);
        assert_eq!(
            rules.term_hit("That is a BADWORD"),
            Some("BadWord".to_string())
        );
        assert_eq!(rules.term_hit("grab a CAFÉ now"), Some("café".to_string()));
        // zero-width char between letters must not evade the match
        assert_eq!(
            rules.term_hit("bad\u{200B}word"),
            Some("BadWord".to_string())
        );
        assert_eq!(rules.term_hit("nothing here"), None);
    }
}
