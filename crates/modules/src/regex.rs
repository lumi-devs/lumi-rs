//! Static-regex compilation with loud failure.
//!
//! Filters and matchers compile constant patterns once. A pattern that fails to
//! compile must never disable a rule silently: the error is logged once at
//! `error!` and the caller skips that one rule. Every pattern is covered by a
//! unit test so a bad edit is caught before it ships rather than at runtime.

use std::sync::OnceLock;

use regex::Regex;

/// Compile a constant pattern, logging (not swallowing) a failure.
#[must_use]
pub fn compile(pattern: &str) -> Option<Regex> {
    match Regex::new(pattern) {
        Ok(regex) => Some(regex),
        Err(error) => {
            tracing::error!(%error, %pattern, "static regex failed to compile; rule disabled");
            None
        }
    }
}

/// Lazily compile a constant pattern into a process-wide slot.
#[must_use]
pub fn cached(slot: &'static OnceLock<Option<Regex>>, pattern: &str) -> Option<&'static Regex> {
    slot.get_or_init(|| compile(pattern)).as_ref()
}

#[cfg(test)]
mod tests {
    #[test]
    fn shipped_patterns_compile() {
        for pattern in [
            super::super::patterns::INVITE_PATTERN,
            super::super::patterns::URL_PATTERN,
            super::super::patterns::CUSTOM_EMOJI_PATTERN,
            super::super::patterns::TARGET_PATTERN,
        ] {
            assert!(
                regex::Regex::new(pattern).is_ok(),
                "pattern must compile: {pattern}"
            );
        }
    }
}
