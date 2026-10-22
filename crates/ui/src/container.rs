use serenity::all::{
    CreateActionRow, CreateContainer, CreateContainerComponent, CreateMediaGallery, CreateSection,
    CreateSectionAccessory, CreateSectionComponent, CreateSeparator, CreateTextDisplay,
    CreateThumbnail, CreateUnfurledMediaItem, SeparatorSpacingSize,
};

use crate::panels::{MAX_ACTION_ROWS, clamp_action_row};

/// Container builder for Discord Components V2.
#[derive(Clone, Debug)]
pub struct Container {
    inner: CreateContainer<'static>,
    action_rows: usize,
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

impl Container {
    /// Create an empty container.
    pub fn new() -> Self {
        Self {
            inner: CreateContainer::new(Vec::new()),
            action_rows: 0,
        }
    }

    /// Append a text display component.
    #[must_use]
    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.inner = self
            .inner
            .add_component(CreateContainerComponent::TextDisplay(text_display(content)));
        self
    }

    /// Append a section component.
    #[must_use]
    pub fn section(mut self, section: CreateSection<'static>) -> Self {
        self.inner = self
            .inner
            .add_component(CreateContainerComponent::Section(section));
        self
    }

    /// Append a separator component.
    #[must_use]
    pub fn separator(mut self, divider: bool, spacing: SeparatorSpacingSize) -> Self {
        self.inner = self
            .inner
            .add_component(CreateContainerComponent::Separator(
                CreateSeparator::new().divider(divider).spacing(spacing),
            ));
        self
    }

    /// Add an action row, enforcing Discord's global row and component clamp.
    #[must_use]
    pub fn action_row(mut self, row: CreateActionRow<'static>) -> Self {
        if self.action_rows >= MAX_ACTION_ROWS {
            tracing::warn!(
                max = MAX_ACTION_ROWS,
                "action-row limit exceeded; dropping extra row"
            );
            return self;
        }
        self.action_rows += 1;
        self.inner = self
            .inner
            .add_component(CreateContainerComponent::ActionRow(clamp_action_row(row)));
        self
    }

    /// Append a media gallery component.
    #[must_use]
    pub fn media_gallery(mut self, gallery: CreateMediaGallery<'static>) -> Self {
        self.inner = self
            .inner
            .add_component(CreateContainerComponent::MediaGallery(gallery));
        self
    }

    /// Convert into a serenity container builder.
    pub fn into_create(self) -> CreateContainer<'static> {
        self.inner
    }
}

/// Build a text display component.
pub fn text_display(content: impl Into<String>) -> CreateTextDisplay<'static> {
    CreateTextDisplay::new(content.into())
}

/// Build a section component with a thumbnail accessory.
pub fn section_with_thumbnail(
    components: Vec<CreateSectionComponent<'static>>,
    url: impl Into<String>,
) -> CreateSection<'static> {
    CreateSection::new(
        components,
        CreateSectionAccessory::Thumbnail(CreateThumbnail::new(CreateUnfurledMediaItem::new(
            url.into(),
        ))),
    )
}
