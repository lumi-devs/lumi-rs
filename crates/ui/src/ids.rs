//! One custom-id scheme for every interactive component.
//!
//! A custom id is `prefix:part:part:…`. Parsing is lossless and total: a
//! malformed id yields `None` or a default page, never a panic.

/// Build `prefix:part:part…` (or just `prefix` when there are no parts).
#[must_use]
pub fn build(prefix: &str, parts: &[&str]) -> String {
    if parts.is_empty() {
        prefix.to_string()
    } else {
        format!("{prefix}:{}", parts.join(":"))
    }
}

/// A custom id split into the segments that follow its prefix.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CustomId {
    parts: Vec<String>,
}

impl CustomId {
    /// Parse `input` against `prefix`; `None` when the prefix does not match.
    #[must_use]
    pub fn parse(input: &str, prefix: &str) -> Option<Self> {
        let mut segments = input.split(':');
        if segments.next()? != prefix {
            return None;
        }
        Some(Self {
            parts: segments.map(str::to_string).collect(),
        })
    }

    /// The first segment, e.g. the action verb.
    #[must_use]
    pub fn head(&self) -> Option<&str> {
        self.get(0)
    }

    /// Segment at `index`, or `None` when out of range.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&str> {
        self.parts.get(index).map(String::as_str)
    }

    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.parts
    }

    #[must_use]
    pub fn last(&self) -> Option<&str> {
        self.parts.last().map(String::as_str)
    }

    /// Segments from `from` onward, joined by `:` (empty past the end).
    #[must_use]
    pub fn tail(&self, from: usize) -> String {
        self.parts
            .get(from..)
            .map_or_else(String::new, |rest| rest.join(":"))
    }

    /// `index` parsed as a page number, defaulting to `0`.
    #[must_use]
    pub fn page(&self, index: usize) -> usize {
        self.parse_usize(index).unwrap_or(0)
    }

    #[must_use]
    pub fn parse_usize(&self, index: usize) -> Option<usize> {
        self.get(index)?.parse().ok()
    }

    #[must_use]
    pub fn parse_u64(&self, index: usize) -> Option<u64> {
        self.get(index)?.parse().ok()
    }

    #[must_use]
    pub fn parse_i32(&self, index: usize) -> Option<i32> {
        self.get(index)?.parse().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_and_parses_round_trip() {
        let id = build("lumi", &["cfg", "open", "economy"]);
        assert_eq!(id, "lumi:cfg:open:economy");
        let Some(parsed) = CustomId::parse(&id, "lumi") else {
            panic!("prefix matches");
        };
        assert_eq!(parsed.head(), Some("cfg"));
        assert_eq!(parsed.get(2), Some("economy"));
        assert_eq!(parsed.tail(1), "open:economy");
    }

    #[test]
    fn malformed_ids_never_panic() {
        assert!(CustomId::parse("other:x", "lumi").is_none());
        assert!(CustomId::parse("", "lumi").is_none());
        let Some(parsed) = CustomId::parse("lumi", "lumi") else {
            panic!("bare prefix parses");
        };
        assert_eq!(parsed.head(), None);
        assert_eq!(parsed.page(3), 0);
        assert_eq!(parsed.tail(1), "");
        assert_eq!(parsed.last(), None);
    }

    #[test]
    fn parses_numbers_lossily() {
        let Some(parsed) = CustomId::parse("ping:p_1:7:notanumber", "ping") else {
            panic!("parses");
        };
        assert_eq!(parsed.parse_u64(1), Some(7));
        assert_eq!(parsed.parse_i32(0), None);
        assert_eq!(parsed.page(3), 0);
    }
}
