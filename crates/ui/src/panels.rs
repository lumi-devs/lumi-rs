use serenity::all::{
    ButtonStyle, CreateActionRow, CreateButton, CreateSelectMenu, CreateSelectMenuKind,
    CreateSelectMenuOption, CreateSection, CreateSectionAccessory, CreateSectionComponent,
    CreateTextDisplay, ReactionType,
};

/// Parse an emoji string into a component emoji.
pub fn component_emoji(name: &str) -> Option<ReactionType> {
    ReactionType::try_from(name).ok()
}

/// An action row of buttons.
pub fn action_row_of(buttons: Vec<CreateButton<'static>>) -> CreateActionRow<'static> {
    CreateActionRow::buttons(buttons)
}

/// Text with a trailing button accessory.
pub fn setting_row(
    text: impl Into<String>,
    button: CreateButton<'static>,
) -> CreateSection<'static> {
    CreateSection::new(
        vec![CreateSectionComponent::TextDisplay(CreateTextDisplay::new(
            text.into(),
        ))],
        CreateSectionAccessory::Button(button),
    )
}

/// A string-select action row.
pub fn string_select(
    custom_id: impl Into<String>,
    placeholder: impl Into<String>,
    options: Vec<(String, String)>,
) -> CreateActionRow<'static> {
    let opts = options
        .into_iter()
        .map(|(label, value)| CreateSelectMenuOption::new(label, value))
        .collect::<Vec<_>>();
    CreateActionRow::SelectMenu(
        CreateSelectMenu::new(
            custom_id.into(),
            CreateSelectMenuKind::String {
                options: opts.into(),
            },
        )
        .placeholder(placeholder.into()),
    )
}

/// A user-select action row.
pub fn user_select(
    custom_id: impl Into<String>,
    placeholder: impl Into<String>,
) -> CreateActionRow<'static> {
    CreateActionRow::SelectMenu(
        CreateSelectMenu::new(
            custom_id.into(),
            CreateSelectMenuKind::User {
                default_users: None,
            },
        )
        .placeholder(placeholder.into()),
    )
}

/// A role-select action row.
pub fn role_select(
    custom_id: impl Into<String>,
    placeholder: impl Into<String>,
) -> CreateActionRow<'static> {
    CreateActionRow::SelectMenu(
        CreateSelectMenu::new(
            custom_id.into(),
            CreateSelectMenuKind::Role {
                default_roles: None,
            },
        )
        .placeholder(placeholder.into()),
    )
}

/// A single secondary "Back" button row.
pub fn back_button(custom_id: impl Into<String>) -> CreateActionRow<'static> {
    action_row_of(vec![
        CreateButton::new(custom_id.into())
            .label("Back")
            .emoji('⬅')
            .style(ButtonStyle::Secondary),
    ])
}

/// A prev / indicator / next pagination row, or `None` when there is one page.
pub fn pagination_row(
    custom_id_prefix: &str,
    page: usize,
    total_pages: usize,
) -> Option<CreateActionRow<'static>> {
    if total_pages < 2 {
        return None;
    }
    let prev = CreateButton::new(format!("{custom_id_prefix}:{}", page.saturating_sub(1)))
        .label("Prev")
        .emoji('◀')
        .style(ButtonStyle::Secondary)
        .disabled(page == 0);
    let next = CreateButton::new(format!("{custom_id_prefix}:{}", page + 1))
        .label("Next")
        .emoji('▶')
        .style(ButtonStyle::Secondary)
        .disabled(page + 1 >= total_pages);
    let indicator = CreateButton::new(format!("{custom_id_prefix}:indicator"))
        .label(format!("{}/{}", page + 1, total_pages))
        .style(ButtonStyle::Secondary)
        .disabled(true);
    Some(action_row_of(vec![prev, indicator, next]))
}
