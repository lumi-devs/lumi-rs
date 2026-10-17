//! Text helpers, ported 1:1 from lumi's `src/lib/ui/Layout.ts`.

/// `🟢/🟡/🔴/⚪/🔘 \`LABEL\`` — badge for a status word.
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
    match icon {
        Some(icon) => format!("-# {icon} {text}"),
        None => format!("-# {text}"),
    }
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

/// Breadcrumb header for the top of a card (`""` when there are no crumbs).
pub fn format_breadcrumb_header(crumbs: &[String]) -> String {
    if crumbs.is_empty() {
        String::new()
    } else {
        format_breadcrumbs(crumbs, " \u{276f} ")
    }
}

/// Trailing hint for a paginated footer: an item count or a free-form string.
pub enum PageFooterHint {
    Count(usize),
    Hint(String),
}

/// `Page N of M · …`.
pub fn format_page_footer(page_index: usize, total_pages: usize, hint: Option<PageFooterHint>) -> String {
    let page = format!("Page {} of {}", page_index + 1, total_pages.max(1));
    match hint {
        Some(PageFooterHint::Count(n)) => {
            format!("{page} \u{b7} {n} item{}", if n == 1 { "" } else { "s" })
        }
        Some(PageFooterHint::Hint(h)) if !h.is_empty() => format!("{page} \u{b7} {h}"),
        _ => page,
    }
}
