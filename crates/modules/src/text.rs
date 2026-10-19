//! Small text helpers shared by module cards and logs.

/// Truncate to `max` characters on a char boundary, appending an ellipsis when
/// shortened.
#[must_use]
pub fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::cut;

    #[test]
    fn cut_respects_char_boundaries() {
        assert_eq!(cut("hello", 10), "hello");
        assert_eq!(cut("hello", 5), "hello");
        assert_eq!(cut("hello", 4), "hel…");
        assert_eq!(cut("héllo", 3), "hé…");
    }
}
