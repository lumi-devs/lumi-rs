//! Small text helpers shared by module cards and logs.

/// Truncate to `max` characters on a char boundary, appending an ellipsis when
/// shortened.
#[must_use]
pub fn cut(text: &str, max: usize) -> String {
    crate::shared::ellipsize(text, max)
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
