use serenity::all::{
    Colour, CreateActionRow, CreateContainer, CreateContainerComponent, CreateMediaGallery,
    CreateSection, CreateSectionAccessory, CreateSectionComponent, CreateSeparator,
    CreateTextDisplay, CreateThumbnail, CreateUnfurledMediaItem, SeparatorSpacingSize,
};

/// Thin composition helper over `CreateContainer`, mirroring lumi's `ContainerBuilder`.
#[derive(Clone, Debug)]
pub struct Container {
    inner: CreateContainer<'static>,
}

impl Default for Container {
    fn default() -> Self {
        Self::new()
    }
}

impl Container {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: CreateContainer::new(Vec::new()),
        }
    }

    #[must_use]
    pub fn accent(mut self, colour: Colour) -> Self {
        self.inner = self.inner.accent_colour(colour);
        self
    }

    #[must_use]
    pub fn text(mut self, content: impl Into<String>) -> Self {
        self.inner = self
            .inner
            .add_component(CreateContainerComponent::TextDisplay(text_display(content)));
        self
    }

    #[must_use]
    pub fn section(mut self, section: CreateSection<'static>) -> Self {
        self.inner = self
            .inner
            .add_component(CreateContainerComponent::Section(section));
        self
    }

    #[must_use]
    pub fn separator(mut self, divider: bool, spacing: SeparatorSpacingSize) -> Self {
        self.inner = self
            .inner
            .add_component(CreateContainerComponent::Separator(
                CreateSeparator::new().divider(divider).spacing(spacing),
            ));
        self
    }

    #[must_use]
    pub fn action_row(mut self, row: CreateActionRow<'static>) -> Self {
        self.inner = self
            .inner
            .add_component(CreateContainerComponent::ActionRow(row));
        self
    }

    #[must_use]
    pub fn media_gallery(mut self, gallery: CreateMediaGallery<'static>) -> Self {
        self.inner = self
            .inner
            .add_component(CreateContainerComponent::MediaGallery(gallery));
        self
    }

    pub fn into_create(self) -> CreateContainer<'static> {
        self.inner
    }
}

/// `TextDisplay` from any string-like value.
pub fn text_display(content: impl Into<String>) -> CreateTextDisplay<'static> {
    CreateTextDisplay::new(content.into())
}

/// A `Section` with a thumbnail accessory.
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

/// A `Small`-spaced separator.
pub fn separator_small(divider: bool) -> CreateSeparator {
    CreateSeparator::new()
        .divider(divider)
        .spacing(SeparatorSpacingSize::Small)
}
