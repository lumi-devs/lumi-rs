use aho_corasick::{AhoCorasick, MatchKind};
use regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterVerdict {
    Clean,
    BlockWord(String),
    DiscordInvite(String),
    MassMention(usize),
    Phishing(String),
}

impl FilterVerdict {
    pub fn is_clean(&self) -> bool {
        matches!(self, Self::Clean)
    }
}

pub struct ContentFilter {
    banned_words: AhoCorasick,
    invite_re: Regex,
    phishing_re: Regex,
    mass_mention_threshold: usize,
}

impl std::fmt::Debug for ContentFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContentFilter")
            .field("mass_mention_threshold", &self.mass_mention_threshold)
            .finish_non_exhaustive()
    }
}

impl ContentFilter {
    pub fn new(
        banned_words: &[&str],
        mass_mention_threshold: usize,
    ) -> Result<Self, crate::error::LumiError> {
        let ac = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .match_kind(MatchKind::LeftmostFirst)
            .build(banned_words)
            .map_err(|e| crate::error::LumiError::Other(e.to_string()))?;

        let invite_re = Regex::new(
            r"(?i)(?:discord\.(?:gg|com/invite)|discordapp\.com/invite)/([a-zA-Z0-9-]+)",
        )
        .map_err(|e| crate::error::LumiError::Other(e.to_string()))?;

        let phishing_re = Regex::new(
            r"(?i)https?://(?:[a-zA-Z0-9-]+\.)*(?:steamcommunit[a-z]|discord-nitro|dlscord|discorcl|free-nitro)[a-zA-Z0-9-]*\.[a-z]{2,}(?:/[^\s]*)?",
        )
        .map_err(|e| crate::error::LumiError::Other(e.to_string()))?;

        Ok(Self {
            banned_words: ac,
            invite_re,
            phishing_re,
            mass_mention_threshold,
        })
    }

    pub fn check_content(&self, _guild_id: u64, content: &str, _author_id: u64) -> FilterVerdict {
        if let Some(m) = self.phishing_re.find(content) {
            return FilterVerdict::Phishing(m.as_str().to_string());
        }

        if let Some(m) = self.invite_re.find(content) {
            return FilterVerdict::DiscordInvite(m.as_str().to_string());
        }

        if let Some(m) = self.banned_words.find(content) {
            let matched_text = &content[m.start()..m.end()];
            return FilterVerdict::BlockWord(matched_text.to_string());
        }

        let mention_count = count_mentions(content);
        if self.mass_mention_threshold > 0 && mention_count >= self.mass_mention_threshold {
            return FilterVerdict::MassMention(mention_count);
        }

        FilterVerdict::Clean
    }
}

impl Default for ContentFilter {
    fn default() -> Self {
        // shortcut: static default filters for bootstrap, per-guild dynamic rules to be added later
        Self::new(&["discord.gg/scam", "grabify.link", "iplogger.org"], 5)
            .unwrap_or_else(|_| {
                let ac = AhoCorasick::builder()
                    .build(["grabify.link"])
                    .expect("static pattern must compile");
                let invite_re = Regex::new(r"discord\.gg/[a-zA-Z0-9-]+")
                    .expect("static regex must compile");
                let phishing_re = Regex::new(r"https?://steamcommunit[a-z]\.[a-z]{2,}")
                    .expect("static regex must compile");
                Self {
                    banned_words: ac,
                    invite_re,
                    phishing_re,
                    mass_mention_threshold: 5,
                }
            })
    }
}

fn count_mentions(content: &str) -> usize {
    let mut count = 0;
    let bytes = content.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' && i + 2 < bytes.len() {
            let next = bytes[i + 1];
            if next == b'@' {
                if let Some(end) = bytes[i + 2..].iter().position(|&b| b == b'>') {
                    count += 1;
                    i += 2 + end + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_clean() {
        let filter = ContentFilter::default();
        let res = filter.check_content(1, "hello world", 2);
        assert_eq!(res, FilterVerdict::Clean);
    }

    #[test]
    fn test_filter_banned_word() {
        let filter = ContentFilter::new(&["badword"], 5).unwrap();
        let res = filter.check_content(1, "This is BadWord here", 2);
        assert_eq!(res, FilterVerdict::BlockWord("BadWord".to_string()));
    }

    #[test]
    fn test_filter_invite() {
        let filter = ContentFilter::default();
        let res = filter.check_content(1, "join here https://discord.gg/xyz123", 2);
        assert_eq!(
            res,
            FilterVerdict::DiscordInvite("discord.gg/xyz123".to_string())
        );
    }

    #[test]
    fn test_filter_phishing() {
        let filter = ContentFilter::default();
        let res = filter.check_content(1, "free nitro at https://discord-nitro.com/free", 2);
        assert_eq!(
            res,
            FilterVerdict::Phishing("https://discord-nitro.com/free".to_string())
        );
    }

    #[test]
    fn test_filter_mass_mentions() {
        let filter = ContentFilter::default();
        let res = filter.check_content(1, "<@123> <@!456> <@&789> <@111> <@222>", 2);
        assert_eq!(res, FilterVerdict::MassMention(5));
    }
}
