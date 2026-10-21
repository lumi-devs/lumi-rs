//! Text helpers, ported 1:1 from lumi's `src/lib/ui/Layout.ts`.

/// Status glyph + `LABEL` code — a badge for a status word.
pub fn format_status_badge(status: &str, label: Option<&str>) -> String {
    let icon = match status.to_ascii_lowercase().as_str() {
        "online" | "success" | "enabled" | "active" => "\u{1f7e2}",
        "idle" | "warning" | "pending" => "\u{1f7e1}",
        "dnd" | "error" | "disabled" | "failed" => "\u{1f534}",
        "offline" | "inactive" => "\u{26aa}",
        _ => "\u{1f518}",
    };
    let upper = status.to_uppercase();
    let text = label.unwrap_or(&upper);
    format!("{icon} `{text}`")
}

/// `-# text` (optionally prefixed with an icon).
pub fn format_subtitle(text: &str, icon: Option<&str>) -> String {
    icon.map_or_else(|| format!("-# {text}"), |icon| format!("-# {icon} {text}"))
}

/// Last crumb bold, crumbs joined by `separator`.
pub fn format_breadcrumbs(crumbs: &[String], separator: &str) -> String {
    let last = crumbs.len().saturating_sub(1);
    crumbs
        .iter()
        .enumerate()
        .map(|(i, crumb)| {
            if i == last {
                format!("**{crumb}**")
            } else {
                crumb.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(separator)
}

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
