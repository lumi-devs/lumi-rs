//! Components V2 card builders, ported from lumi's `src/lib/ui/Cards.ts`.
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CardKind {
    /// `primary` — no accent bar (blank by default).
    #[default]
    Primary,
    Info,
    Success,
    Warning,
    Error,
    Gold,
    Purple,
}

impl CardKind {
    /// Accent colour, or `None` for `Primary` (matches `resolveCardColor`).
    #[must_use]
    pub const fn accent(self) -> Option<Colour> {
        match self {
            CardKind::Primary => None,
            CardKind::Info => Some(Colour::new(0x004c_6ef5)),
            CardKind::Success => Some(Colour::new(0x0012_b886)),
            CardKind::Warning => Some(Colour::new(0x00f5_9f00)),
            CardKind::Error => Some(Colour::new(0x00fa_5252)),
            CardKind::Gold => Some(Colour::new(0x00ff_d43b)),
            CardKind::Purple => Some(Colour::new(0x0097_75fa)),
        }
    }
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug)]
pub struct CardOptions {
    pub subtitle: Option<String>,
    pub breadcrumbs: Vec<String>,
    pub status_badge: Option<(String, Option<String>)>,
    pub footer: Option<String>,
    pub thumbnail_url: Option<String>,
    pub divider: bool,
    pub hide_title: bool,
    pub no_accent: bool,
    pub header_images: Vec<String>,
    pub separator_above_action_rows: bool,
    pub sections: Vec<CreateSection<'static>>,
    pub action_rows: Vec<CreateActionRow<'static>>,
    pub media_gallery: Option<CreateMediaGallery<'static>>,
}

impl Default for CardOptions {
    fn default() -> Self {
        Self {
            subtitle: None,
            breadcrumbs: Vec::new(),
            status_badge: None,
            footer: None,
            thumbnail_url: None,
            divider: true,
            hide_title: false,
            no_accent: false,
            header_images: Vec::new(),
            separator_above_action_rows: false,
            sections: Vec::new(),
            action_rows: Vec::new(),
            media_gallery: None,
        }
    }
}

/// A built card: the V2 components plus whether it should be ephemeral.
#[derive(Clone, Debug)]
pub struct CardReply {
    pub components: Vec<CreateComponent<'static>>,
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
    }

    /// Render into a raw channel message (V2 flag set, no-ping mentions).
    pub fn message(&self) -> CreateMessage<'static> {
        CreateMessage::new()
            .components(self.components.clone())
            .allowed_mentions(CreateAllowedMentions::new())
            .flags(self.flags())
    }
}

#[derive(Clone, Debug)]
pub struct Card {
    pub title: String,
    pub body: Vec<String>,
    pub accent: Option<Colour>,
    pub options: CardOptions,
    pub ephemeral: bool,
}

impl Card {
    pub fn new(title: impl Into<String>, body: impl Into<String>) -> Self {
        Card::lines(title, vec![body.into()])
    }

    pub fn lines(title: impl Into<String>, body: Vec<String>) -> Self {
        Self {
            title: title.into(),
            body,
            accent: None,
            options: CardOptions::default(),
            ephemeral: false,
        }
    }

    pub fn info(title: impl Into<String>, body: impl Into<String>) -> Self {
        Card::new(title, body).kind(CardKind::Info)
    }

    pub fn success(title: impl Into<String>, body: impl Into<String>) -> Self {
        Card::new(title, body).kind(CardKind::Success)
    }

    pub fn error(title: impl Into<String>, body: impl Into<String>) -> Self {
        Card::new(title, body).kind(CardKind::Error)
    }

    pub fn warning(title: impl Into<String>, body: impl Into<String>) -> Self {
        Card::new(title, body).kind(CardKind::Warning)
    }

    #[must_use]
    pub const fn kind(mut self, kind: CardKind) -> Self {
        self.accent = kind.accent();
        self
    }

    #[must_use]
    pub const fn accent(mut self, colour: Option<Colour>) -> Self {
        self.accent = colour;
        self
    }

    #[must_use]
    pub fn options(mut self, options: CardOptions) -> Self {
        self.options = options;
        self
    }

    #[must_use]
    pub const fn ephemeral(mut self) -> Self {
        self.ephemeral = true;
        self
    }

    #[must_use]
    pub fn into_reply(&self) -> CardReply {
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
            children.push(separator(opts.divider));
        }

        for section in &opts.sections {
            children.push(CreateContainerComponent::Section(section.clone()));
        }

        let thumb_url = if opts.sections.is_empty() {
            opts.thumbnail_url.as_ref()
        } else {
            None
        };

        if let Some(url) = thumb_url {
            let non_empty: Vec<&String> = self.body.iter().filter(|p| !p.is_empty()).collect();
            let first = non_empty
                .first()
                .map_or_else(|| self.title.clone(), |p| (*p).clone());
            let section = CreateSection::new(
                vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
                    first,
                ))],
                CreateSectionAccessory::Thumbnail(CreateThumbnail::new(
                    CreateUnfurledMediaItem::new(url.clone()),
                )),
            );
            children.push(CreateContainerComponent::Section(section));
            for part in non_empty.into_iter().skip(1) {
                children.push(separator(true));
                children.push(text(part.clone()));
            }
        } else {
            for (i, part) in self.body.iter().enumerate() {
                if i > 0 {
                    children.push(separator(true));
                }
                if !part.is_empty() {
                    children.push(text(part.clone()));
                }
            }
        }

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

        // Central action-row clamp, mirroring TS `buildSafeActionRows`.
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
    let mut kept: Vec<String> = Vec::new();
    let mut used = 0usize;

    for (i, line) in lines.iter().enumerate() {
        let notice = format!("-# ...and {} more item(s).", lines.len() - i);
        let len = line.chars().count();
        let cost = if kept.is_empty() { len } else { len + 1 };

        if used + cost > budget.saturating_sub(notice.chars().count() + 1) {
            kept.push(notice);
            break;
        }
        kept.push(line.clone());
        used += cost;
    }

    kept.join("\n").chars().take(budget).collect()
}

/// Truncate to `max` chars, appending `…` when cut. Char-based, so never splits
/// a multi-byte codepoint.
#[must_use]
pub fn cut_text(text: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut truncated: String = text.chars().take(max.saturating_sub(1)).collect();
    truncated.push('\u{2026}');
    truncated
}

/// Bulleted list card: max 25 visible items, each truncated to 200 chars.
#[must_use]
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
        assert_eq!(format_status_badge("x", Some("Custom")), "\u{1f518} `Custom`");
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
        use crate::panels::{action_row_of, MAX_ACTION_ROWS, MAX_ROW_COMPONENTS};
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
        let rows = json["components"]
            .as_array()
            .map_or(0, |components| {
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
