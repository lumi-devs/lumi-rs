//! Text layout helpers for Components V2.

/// Status glyph + `LABEL` code — a badge for a status word.
pub fn format_status_badge(status: &str, label: Option<&str>) -> String {
    let icon = if status.eq_ignore_ascii_case("online")
        || status.eq_ignore_ascii_case("success")
        || status.eq_ignore_ascii_case("enabled")
        || status.eq_ignore_ascii_case("active")
    {
        "\u{1f7e2}"
    } else if status.eq_ignore_ascii_case("idle")
        || status.eq_ignore_ascii_case("warning")
        || status.eq_ignore_ascii_case("pending")
    {
        "\u{1f7e1}"
    } else if status.eq_ignore_ascii_case("dnd")
        || status.eq_ignore_ascii_case("error")
        || status.eq_ignore_ascii_case("disabled")
        || status.eq_ignore_ascii_case("failed")
    {
        "\u{1f534}"
    } else if status.eq_ignore_ascii_case("offline")
        || status.eq_ignore_ascii_case("inactive")
    {
        "\u{26aa}"
    } else {
        "\u{1f518}"
    };
    if let Some(text) = label {
        format!("{icon} `{text}`")
    } else {
        let upper = status.to_uppercase();
        format!("{icon} `{upper}`")
    }
}

/// `-# text` (optionally prefixed with an icon).
pub fn format_subtitle(text: &str, icon: Option<&str>) -> String {
    icon.map_or_else(|| format!("-# {text}"), |icon| format!("-# {icon} {text}"))
}

/// Last crumb bold, crumbs joined by `separator`.
pub fn format_breadcrumbs(crumbs: &[String], separator: &str) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let last = crumbs.len().saturating_sub(1);
    for (i, crumb) in crumbs.iter().enumerate() {
        if i > 0 {
            out.push_str(separator);
        }
        if i == last {
            let _ = write!(out, "**{crumb}**");
        } else {
            out.push_str(crumb);
        }
    }
    out
}

/// Separator string between breadcrumb segments.
pub const BREADCRUMB_SEPARATOR: &str = " \u{276f} ";

/// Breadcrumb header for the top of a card (`""` when there are no crumbs).
pub fn format_breadcrumb_header(crumbs: &[String]) -> String {
    if crumbs.is_empty() {
        String::new()
    } else {
        format_breadcrumbs(crumbs, BREADCRUMB_SEPARATOR)
    }
}

/// `Page N of M` (optionally suffixed with ` · hint`).
pub fn format_page_footer(page_index: usize, total_pages: usize, hint: Option<&str>) -> String {
    let page = format!(
        "Page {} of {}",
        page_index.saturating_add(1),
        total_pages.max(1)
    );
    match hint {
        Some(hint) if !hint.is_empty() => format!("{page} \u{b7} {hint}"),
        _ => page,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_footer_handles_empty_hint() {
        assert_eq!(format_page_footer(0, 0, None), "Page 1 of 1");
        assert_eq!(format_page_footer(2, 5, Some("")), "Page 3 of 5");
        assert_eq!(
            format_page_footer(2, 5, Some("12 items")),
            "Page 3 of 5 · 12 items"
        );
    }
}
