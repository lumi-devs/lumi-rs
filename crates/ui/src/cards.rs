//! Components V2 card builders.
//! Every reply is a V2 `Container`; there are no embeds.

use serenity::all::{
    Colour, CreateActionRow, CreateAllowedMentions, CreateComponent, CreateContainer,
    CreateContainerComponent, CreateMediaGallery, CreateMediaGalleryItem, CreateMessage,
    CreateSection, CreateSectionAccessory, CreateSectionComponent, CreateSeparator,
    CreateTextDisplay, CreateThumbnail, CreateUnfurledMediaItem, MessageFlags,
    SeparatorSpacingSize,
};

use crate::layout::{format_breadcrumb_header, format_status_badge, format_subtitle};
use crate::panels::clamp_action_rows;

/// Discord's hard limit for a single text display.
pub const TEXT_DISPLAY_LIMIT: usize = 4000;

/// Card style kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CardKind {
    /// `primary` — no accent bar (blank by default).
    #[default]
    Primary,
    /// Information card.
    Info,
    /// Success card.
    Success,
    /// Warning card.
    Warning,
    /// Error card.
    Error,
    /// Gold accent card.
    Gold,
    /// Purple accent card.
    Purple,
}

impl CardKind {
    /// Accent colour, or `None` for `Primary` (matches `resolveCardColor`).
    pub const fn accent(self) -> Option<Colour> {
        match self {
            Self::Primary => None,
            Self::Info => Some(Colour::new(0x004c_6ef5)),
            Self::Success => Some(Colour::new(0x0012_b886)),
            Self::Warning => Some(Colour::new(0x00f5_9f00)),
            Self::Error => Some(Colour::new(0x00fa_5252)),
            Self::Gold => Some(Colour::new(0x00ff_d43b)),
            Self::Purple => Some(Colour::new(0x0097_75fa)),
        }
    }
}

/// Optional settings for a card.
#[derive(Clone, Debug, Default)]
pub struct CardOptions {
    /// Card subtitle.
    pub subtitle: Option<String>,
    /// Breadcrumb hierarchy.
    pub breadcrumbs: Vec<String>,
    /// Status badge status and optional label.
    pub status_badge: Option<(String, Option<String>)>,
    /// Card footer text.
    pub footer: Option<String>,
    /// Thumbnail image URL.
    pub thumbnail_url: Option<String>,
    /// Whether to suppress the title.
    pub hide_title: bool,
    /// Whether to suppress the accent colour.
    pub no_accent: bool,
    /// Header image URLs.
    pub header_images: Vec<String>,
    /// Whether to place a separator above action rows.
    pub separator_above_action_rows: bool,
    /// Additional card sections.
    pub sections: Vec<CreateSection<'static>>,
    /// Action row components.
    pub action_rows: Vec<CreateActionRow<'static>>,
    /// Media gallery component.
    pub media_gallery: Option<CreateMediaGallery<'static>>,
}

/// A built card: the V2 components plus whether it should be ephemeral.
#[derive(Clone, Debug)]
pub struct CardReply {
    /// Components belonging to the reply.
    pub components: Vec<CreateComponent<'static>>,
    /// Whether the reply is ephemeral.
    pub ephemeral: bool,
}

impl CardReply {
    fn flags(&self) -> MessageFlags {
        let mut flags = MessageFlags::IS_COMPONENTS_V2;
        if self.ephemeral {
            flags |= MessageFlags::EPHEMERAL;
        }
        flags
    }

    /// Render into a poise reply: V2 flag set, no-ping allowed mentions.
    pub fn reply(&self) -> poise::CreateReply<'static> {
        poise::CreateReply::default()
            .components(self.components.clone())
            .allowed_mentions(CreateAllowedMentions::new())
            .flags(self.flags())
            .reply(true)
    }

    /// Render into a raw channel message (V2 flag set, no-ping mentions).
    pub fn message(&self) -> CreateMessage<'static> {
        CreateMessage::new()
            .components(self.components.clone())
            .allowed_mentions(CreateAllowedMentions::new())
            .flags(self.flags())
    }
}

/// Components V2 card builder.
#[derive(Clone, Debug)]
pub struct Card {
    /// Card title text.
    pub title: String,
    /// Card body lines.
    pub body: Vec<String>,
    /// Accent colour override.
    pub accent: Option<Colour>,
    /// Optional card settings.
    pub options: CardOptions,
    /// Whether the card is ephemeral.
    pub ephemeral: bool,
}

impl Card {
    /// Create a new card with title and single-line body.
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self::lines(title, vec![body.into()])
    }

    /// Create a new card with title and multi-line body.
    pub fn lines(title: impl Into<String>, body: Vec<String>) -> Self {
        Self {
            title: title.into(),
            body,
            accent: None,
            options: CardOptions::default(),
            ephemeral: false,
        }
    }

    /// Create an info card.
    pub fn info(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self::new(title, body).kind(CardKind::Info)
    }

    /// Create a success card.
    pub fn success(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self::new(title, body).kind(CardKind::Success)
    }

    /// Create an error card.
    pub fn error(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self::new(title, body).kind(CardKind::Error)
    }

    /// Create a warning card.
    pub fn warning(title: impl Into<String>, body: impl Into<String>) -> Self {
        Self::new(title, body).kind(CardKind::Warning)
    }

    /// Set the card style kind.
    #[must_use]
    pub const fn kind(mut self, kind: CardKind) -> Self {
        self.accent = kind.accent();
        self
    }

    /// Set an explicit accent colour.
    #[must_use]
    pub const fn accent(mut self, colour: Option<Colour>) -> Self {
        self.accent = colour;
        self
    }

    /// Set optional card settings.
    #[must_use]
    pub fn options(mut self, options: CardOptions) -> Self {
        self.options = options;
        self
    }

    /// Mark the card as ephemeral.
    #[must_use]
    pub const fn ephemeral(mut self) -> Self {
        self.ephemeral = true;
        self
    }

    /// Build the card into a reply container.
    pub fn into_reply(self) -> CardReply {
        CardReply {
            components: vec![CreateComponent::Container(self.build_container())],
            ephemeral: self.ephemeral,
        }
    }

    fn build_container(&self) -> CreateContainer<'static> {
        let opts = &self.options;
        let mut children: Vec<CreateContainerComponent<'static>> = Vec::new();

        let mut header: Vec<String> = Vec::new();
        if !opts.breadcrumbs.is_empty() {
            header.push(format_breadcrumb_header(&opts.breadcrumbs));
        }
        if let Some((status, label)) = &opts.status_badge {
            header.push(format_status_badge(status, label.as_deref()));
        }
        if !header.is_empty() {
            children.push(text(header.join(" \u{b7} ")));
        }

        if !opts.hide_title && !self.title.is_empty() {
            children.push(text(format!("## {}", self.title)));
            if let Some(subtitle) = &opts.subtitle {
                children.push(text(format_subtitle(subtitle, None)));
            }
            children.push(separator(true));
        }

        for section in &opts.sections {
            children.push(CreateContainerComponent::Section(section.clone()));
        }

        children.extend(body_components(opts, &self.title, &self.body));

        let images: Vec<String> = opts
            .header_images
            .iter()
            .filter(|url| !url.is_empty())
            .take(10)
            .cloned()
            .collect();
        if !images.is_empty() {
            let items = images
                .into_iter()
                .map(|url| CreateMediaGalleryItem::new(CreateUnfurledMediaItem::new(url)))
                .collect::<Vec<_>>();
            children.push(CreateContainerComponent::MediaGallery(
                CreateMediaGallery::new(items),
            ));
        }

        if let Some(gallery) = &opts.media_gallery {
            children.push(CreateContainerComponent::MediaGallery(gallery.clone()));
        }

        if let Some(footer) = &opts.footer {
            children.push(separator(true));
            children.push(text(format!("-# {footer}")));
        }

        let action_rows = clamp_action_rows(opts.action_rows.clone());
        if opts.separator_above_action_rows && !action_rows.is_empty() {
            children.push(CreateContainerComponent::Separator(CreateSeparator::new()));
        }
        for row in action_rows {
            children.push(CreateContainerComponent::ActionRow(row));
        }

        let mut container = CreateContainer::new(children);
        if let Some(accent) = self.accent
            && !opts.no_accent
        {
            container = container.accent_colour(accent);
        }
        container
    }
}

/// Body text, with a thumbnail section when the card has no explicit sections.
fn body_components(
    opts: &CardOptions,
    title: &str,
    body: &[String],
) -> Vec<CreateContainerComponent<'static>> {
    let mut children = Vec::new();
    let non_empty: Vec<&str> = body.iter().map(String::as_str).filter(|p| !p.is_empty()).collect();
    let thumb_url = opts
        .thumbnail_url
        .as_ref()
        .filter(|_| opts.sections.is_empty());

    if let Some(url) = thumb_url {
        let first = non_empty.first().copied().unwrap_or(title);
        let section = CreateSection::new(
            vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
                first.to_string(),
            ))],
            CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(
                url.clone(),
            ))),
        );
        children.push(CreateContainerComponent::Section(section));
        if non_empty.len() > 1 {
            children.push(separator(true));
            children.push(text(non_empty[1..].join("\n")));
        }
    } else {
        let chunk_size = if non_empty.len() > 10 {
            non_empty.len().div_ceil(10)
        } else {
            1
        };
        for (i, chunk) in non_empty.chunks(chunk_size).enumerate() {
            if i > 0 {
                children.push(separator(true));
            }
            children.push(text(chunk.join("\n")));
        }
    }
    children
}

fn text(content: impl Into<String>) -> CreateContainerComponent<'static> {
    CreateContainerComponent::TextDisplay(CreateTextDisplay::new(content.into()))
}

fn separator(divider: bool) -> CreateContainerComponent<'static> {
    CreateContainerComponent::Separator(
        CreateSeparator::new()
            .divider(divider)
            .spacing(SeparatorSpacingSize::Small),
    )
}

/// Keep as many lines as fit in `budget`, ending with a `-# ...and N more` note.
pub fn fit_lines(lines: &[String], budget: usize) -> String {
    let mut kept: Vec<&str> = Vec::new();
    let mut notice_storage = String::new();
    let mut used = 0_usize;

    for (i, line) in lines.iter().enumerate() {
        let remaining = lines.len() - i;
        let digits = usize::try_from(remaining.checked_ilog10().unwrap_or(0)).unwrap_or(0);
        let notice_len = 25 + digits + 1;
        let len = line.chars().count();
        let cost = if kept.is_empty() { len } else { len + 1 };

        if used + cost > budget.saturating_sub(notice_len + 1) {
            use std::fmt::Write;
            let _ = write!(notice_storage, "-# ...and {remaining} more item(s).");
            kept.push(&notice_storage);
            break;
        }
        kept.push(line.as_str());
        used += cost;
    }

    kept.join("\n").chars().take(budget).collect()
}

/// Truncate to `max` chars, appending `…` when cut. Char-based, so never splits
/// a multi-byte codepoint.
pub fn cut_text(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let mut indices = text.char_indices();
    let cut_point = indices.nth(max.saturating_sub(1));
    match cut_point {
        None => text.to_string(),
        Some((byte_idx, _)) => {
            if indices.next().is_none() {
                text.to_string()
            } else {
                let mut truncated = String::with_capacity(byte_idx + '\u{2026}'.len_utf8());
                truncated.push_str(&text[..byte_idx]);
                truncated.push('\u{2026}');
                truncated
            }
        }
    }
}

/// Bulleted list card: max 25 visible items, each truncated to 200 chars.
pub fn make_list_card(title: &str, items: &[String], options: CardOptions) -> CardReply {
    let body = if items.is_empty() {
        vec!["-# *No items to display.*".to_string()]
    } else {
        const MAX_VISIBLE: usize = 25;
        let mut lines: Vec<String> = items
            .iter()
            .take(MAX_VISIBLE)
            .map(|item| format!("\u{2022} {}", cut_text(item, 200)))
            .collect();
        if items.len() > MAX_VISIBLE {
            lines.push(format!(
                "-# ...and {} more item(s).",
                items.len() - MAX_VISIBLE
            ));
        }
        vec![fit_lines(&lines, TEXT_DISPLAY_LIMIT)]
    };
    Card::lines(title, body)
        .kind(CardKind::Info)
        .options(options)
        .into_reply()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::format_status_badge;

    #[test]
    fn primary_has_no_accent() {
        assert!(CardKind::Primary.accent().is_none());
        assert!(CardKind::Error.accent().is_some());
    }

    #[test]
    fn badge_matches_layout_port() {
        assert_eq!(format_status_badge("success", None), "\u{1f7e2} `SUCCESS`");
        assert_eq!(
            format_status_badge("x", Some("Custom")),
            "\u{1f518} `Custom`"
        );
    }

    #[test]
    fn fit_lines_clamps_and_notes() {
        let lines: Vec<String> = (0..5).map(|i| format!("{}{i}", "x".repeat(100))).collect();
        let out = fit_lines(&lines, 300);
        assert!(out.chars().count() <= 300);
        assert!(out.contains("more item(s)."));
    }

    #[test]
    fn oversized_cards_are_clamped() {
        use crate::panels::{MAX_ACTION_ROWS, MAX_ROW_COMPONENTS, action_row_of};
        use serenity::all::CreateButton;
        let rows: Vec<CreateActionRow<'static>> = (0..MAX_ACTION_ROWS + 3)
            .map(|i| {
                action_row_of(
                    (0..MAX_ROW_COMPONENTS + 2)
                        .map(|j| CreateButton::new(format!("b{i}-{j}")))
                        .collect(),
                )
            })
            .collect();
        let card = Card::new("t", "b")
            .options(CardOptions {
                action_rows: rows,
                ..CardOptions::default()
            })
            .into_reply();
        let Ok(json) = serde_json::to_value(&card.components[0]) else {
            panic!("card serializes");
        };
        let rows = json["components"].as_array().map_or(0, |components| {
            components
                .iter()
                .filter(|component| component["type"] == 1)
                .count()
        });
        assert_eq!(rows, MAX_ACTION_ROWS);
    }

    #[test]
    fn cut_text_is_codepoint_safe() {
        assert_eq!(cut_text("short", 10), "short");
        assert_eq!(cut_text("short", 0), "");
        let cut = cut_text("🦀🦀🦀🦀", 3);
        assert_eq!(cut.chars().count(), 3);
        assert!(cut.ends_with('\u{2026}'));
    }
}
