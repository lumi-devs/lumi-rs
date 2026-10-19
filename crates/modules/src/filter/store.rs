use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use regex::RegexBuilder;
use sea_orm::DatabaseConnection;
use serde_json::{Map, Value};

use lumi_kernel::module_state;

const MODULE: &str = "filter";

/// Mirrors lumi's `DefaultCapsMinLength`.
pub const DEFAULT_CAPS_MIN_LENGTH: i64 = 12;
/// Mirrors lumi's `DefaultWarnMessage`.
pub const DEFAULT_WARN_MESSAGE: &str = "{user}, your message was removed for containing {reason}.";
/// Mirrors lumi's `MaxRegexLength`.
const MAX_REGEX_LENGTH: usize = 256;

/// Fully resolved, compiled filter config for one guild. `terms`/allowlists are
/// lowercased for case-insensitive matching.
#[derive(Debug)]
pub struct FilterRules {
    pub terms: Vec<String>,
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
}

impl FilterRules {
    /// Whether any rule could possibly fire; an unconfigured guild is skipped.
    #[must_use]
    pub fn active(&self) -> bool {
        !self.terms.is_empty()
            || !self.regexes.is_empty()
            || self.block_invites
            || self.block_links
            || self.max_mentions > 0
            || self.caps_pct > 0
    }

    pub(crate) fn from_map(map: &Map<String, Value>) -> Self {
        // shortcut: plain lowercase substring match, no NFKD/invisible-char
        // normalisation like lumi; add unicode-normalization if obfuscated terms matter.
        let terms = string_list(map, "terms")
            .iter()
            .map(|term| term.to_lowercase())
            .filter(|term| !term.is_empty())
            .collect();
        let regexes = compile_regexes(&string_list(map, "regex_rules"));
        let warn_message = map
            .get("warn_message")
            .and_then(Value::as_str)
            .map_or_else(|| DEFAULT_WARN_MESSAGE.to_string(), |text| text.to_string());

        Self {
            terms,
            regexes,
            invite_allow: lowercase(string_list(map, "invite_allowlist")),
            block_invites: boolean(map, "block_invites", false),
            link_allow: lowercase(string_list(map, "link_allowlist")),
            block_links: boolean(map, "block_links", false),
            max_mentions: number(map, "max_mentions", 0),
            caps_pct: number(map, "max_caps_percent", 0),
            caps_min_len: number(map, "caps_min_length", DEFAULT_CAPS_MIN_LENGTH).max(1) as usize,
            exempt_roles: id_list(map, "exempt_roles"),
            timeout_minutes: number(map, "timeout_minutes", 0),
            spam_timeout_minutes: number(map, "spam_timeout_minutes", 0),
            invite_timeout_minutes: number(map, "invite_timeout_minutes", 0),
            link_timeout_minutes: number(map, "link_timeout_minutes", 0),
            warn_message,
            log_channel: map.get("log_channel_id").and_then(module_state::parse_id),
        }
    }
}

/// Load the guild's filter rules, reading config once per call and reusing the
/// compiled set while the stored config is unchanged (hash-keyed).
pub async fn rules_for(db: &DatabaseConnection, guild_id: u64) -> Arc<FilterRules> {
    let map = module_state::get_config(db, guild_id, MODULE).await;
    let hash = map_hash(&map);
    if let Some(rules) = cache().get(guild_id, hash) {
        return rules;
    }
    let rules = Arc::new(FilterRules::from_map(&map));
    cache().insert(guild_id, hash, Arc::clone(&rules));
    rules
}

type Entry = (u64, Arc<FilterRules>);

#[derive(Default)]
struct RulesCache {
    entries: Mutex<HashMap<u64, Entry>>,
}

impl RulesCache {
    fn get(&self, guild_id: u64, hash: u64) -> Option<Arc<FilterRules>> {
        let guard = self
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        guard
            .get(&guild_id)
            .filter(|(cached_hash, _)| *cached_hash == hash)
            .map(|(_, rules)| Arc::clone(rules))
    }

    fn insert(&self, guild_id: u64, hash: u64, rules: Arc<FilterRules>) {
        let mut guard = self
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        guard.insert(guild_id, (hash, rules));
    }
}

fn cache() -> &'static RulesCache {
    // shortcut: unbounded per-guild map; move to a bounded moka cache if a
    // process ever serves enough distinct guilds for memory to matter.
    static CACHE: OnceLock<RulesCache> = OnceLock::new();
    CACHE.get_or_init(RulesCache::default)
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
            RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .map_err(|error| {
                    tracing::warn!(%pattern, %error, "filter: skipping invalid regex");
                    error
                })
                .ok()
        })
        .collect()
}

fn string_list(map: &Map<String, Value>, key: &str) -> Vec<String> {
    map.get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn lowercase(items: Vec<String>) -> Vec<String> {
    items.into_iter().map(|item| item.to_lowercase()).collect()
}

fn id_list(map: &Map<String, Value>, key: &str) -> Vec<u64> {
    map.get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(module_state::parse_id).collect())
        .unwrap_or_default()
}

fn boolean(map: &Map<String, Value>, key: &str, fallback: bool) -> bool {
    map.get(key).and_then(Value::as_bool).unwrap_or(fallback)
}

fn number(map: &Map<String, Value>, key: &str, fallback: i64) -> i64 {
    map.get(key)
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        })
        .unwrap_or(fallback)
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
    fn terms_are_lowercased_and_blank_dropped() {
        let rules = FilterRules::from_map(&map(json!({ "terms": ["BadWord", ""] })));
        assert_eq!(rules.terms, vec!["badword"]);
    }
}
