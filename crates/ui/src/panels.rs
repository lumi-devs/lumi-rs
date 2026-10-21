//! Action rows, selects, pagination and the central row clamp.
//! Ported from lumi's `src/lib/ui/Panels.ts`.

use serenity::all::{
    ButtonStyle, ChannelType, CreateActionRow, CreateButton, CreateSection, CreateSectionAccessory,
    CreateSectionComponent, CreateSelectMenu, CreateSelectMenuKind, CreateSelectMenuOption,
    CreateTextDisplay, ReactionType,
};

/// Discord's maximum action rows per message.
pub const MAX_ACTION_ROWS: usize = 5;
/// Discord's maximum components per action row.
pub const MAX_ROW_COMPONENTS: usize = 5;

/// Parse an emoji string into a component emoji.
pub fn component_emoji(name: &str) -> Option<ReactionType> {
    ReactionType::try_from(name).ok()
}

pub fn action_row_of(buttons: Vec<CreateButton<'static>>) -> CreateActionRow<'static> {
    CreateActionRow::buttons(buttons)
}

/// Clamp one row to Discord's per-row component limit.
pub fn clamp_action_row(row: CreateActionRow<'static>) -> CreateActionRow<'static> {
    match row {
        CreateActionRow::Buttons(buttons) if buttons.len() > MAX_ROW_COMPONENTS => {
            tracing::warn!(
                found = buttons.len(),
                max = MAX_ROW_COMPONENTS,
                "action-row component limit exceeded; truncating"
            );
            let mut buttons = buttons.into_owned();
            buttons.truncate(MAX_ROW_COMPONENTS);
            CreateActionRow::buttons(buttons)
        }
        row => row,
    }
}

/// Clamp rows to Discord's limits, mirroring TS `buildSafeActionRows`: at most
/// [`MAX_ACTION_ROWS`] rows, and at most [`MAX_ROW_COMPONENTS`] buttons in each.
#[must_use]
pub fn clamp_action_rows(rows: Vec<CreateActionRow<'static>>) -> Vec<CreateActionRow<'static>> {
    if rows.len() > MAX_ACTION_ROWS {
        tracing::warn!(
            found = rows.len(),
            max = MAX_ACTION_ROWS,
            "action-row limit exceeded; truncating"
        );
    }
    rows.into_iter()
        .take(MAX_ACTION_ROWS)
        .map(clamp_action_row)
        .collect()
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

pub fn string_select(
    custom_id: impl Into<String>,
    placeholder: impl Into<String>,
    options: Vec<(String, String)>,
) -> CreateActionRow<'static> {
    string_select_options(
        custom_id,
        placeholder,
        options
            .into_iter()
            .map(|(label, value)| (label, value, None))
            .collect(),
    )
}

/// A string-select action row with an optional emoji on each option.
pub fn string_select_options(
    custom_id: impl Into<String>,
    placeholder: impl Into<String>,
    options: Vec<(String, String, Option<&str>)>,
) -> CreateActionRow<'static> {
    let opts = options
        .into_iter()
        .map(|(label, value, emoji)| {
            let mut option = CreateSelectMenuOption::new(label, value);
            if let Some(emoji) = emoji.and_then(component_emoji) {
                option = option.emoji(emoji);
            }
            option
        })
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

/// A channel-select action row restricted to `types`.
pub fn channel_select_of_types(
    custom_id: impl Into<String>,
    placeholder: impl Into<String>,
    types: Vec<ChannelType>,
) -> CreateActionRow<'static> {
    CreateActionRow::SelectMenu(
        CreateSelectMenu::new(
            custom_id.into(),
            CreateSelectMenuKind::Channel {
                channel_types: Some(types.into()),
                default_channels: None,
            },
        )
        .placeholder(placeholder.into()),
    )
}

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

/// A channel-select action row (clearing the selection is allowed).
pub fn channel_select(
    custom_id: impl Into<String>,
    placeholder: impl Into<String>,
) -> CreateActionRow<'static> {
    CreateActionRow::SelectMenu(
        CreateSelectMenu::new(
            custom_id.into(),
            CreateSelectMenuKind::Channel {
                channel_types: None,
                default_channels: None,
            },
        )
        .placeholder(placeholder.into())
        .min_values(0),
    )
}

/// A role-select action row allowing up to `max_values` picks.
pub fn role_select_multi(
    custom_id: impl Into<String>,
    placeholder: impl Into<String>,
    max_values: u8,
) -> CreateActionRow<'static> {
    CreateActionRow::SelectMenu(
        CreateSelectMenu::new(
            custom_id.into(),
            CreateSelectMenuKind::Role {
                default_roles: None,
            },
        )
        .placeholder(placeholder.into())
        .min_values(0)
        .max_values(max_values),
    )
}

pub fn back_button(custom_id: impl Into<String>) -> CreateActionRow<'static> {
    action_row_of(vec![
        CreateButton::new(custom_id.into())
            .label("\u{2190} Back")
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
    let mut prev = CreateButton::new(format!("{custom_id_prefix}:{}", page.saturating_sub(1)))
        .label("Prev")
        .style(ButtonStyle::Secondary)
        .disabled(page == 0);
    if let Some(emoji) = component_emoji("\u{2b05}\u{fe0f}") {
        prev = prev.emoji(emoji);
    }
    let mut next = CreateButton::new(format!("{custom_id_prefix}:{}", page.saturating_add(1)))
        .label("Next")
        .style(ButtonStyle::Secondary)
        .disabled(page.saturating_add(1) >= total_pages);
    if let Some(emoji) = component_emoji("\u{27a1}\u{fe0f}") {
        next = next.emoji(emoji);
    }
    let indicator = CreateButton::new(format!("{custom_id_prefix}:indicator"))
        .label(format!("{} / {}", page.saturating_add(1), total_pages))
        .style(ButtonStyle::Secondary)
        .disabled(true);
    Some(action_row_of(vec![prev, indicator, next]))
}

/// A confirmation row with a Danger Confirm button and a Secondary Cancel button.
pub fn confirm_row(
    confirm_id: impl Into<String>,
    cancel_id: impl Into<String>,
    confirm_label: Option<&str>,
    cancel_label: Option<&str>,
) -> CreateActionRow<'static> {
    action_row_of(vec![
        CreateButton::new(confirm_id.into())
            .label(confirm_label.unwrap_or("Confirm").to_string())
            .style(ButtonStyle::Danger),
        CreateButton::new(cancel_id.into())
            .label(cancel_label.unwrap_or("Cancel").to_string())
            .style(ButtonStyle::Secondary),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_truncates_rows_and_components() {
        let rows: Vec<CreateActionRow<'static>> = (0..MAX_ACTION_ROWS + 2)
            .map(|i| {
                action_row_of(
                    (0..MAX_ROW_COMPONENTS + 2)
                        .map(|j| CreateButton::new(format!("b{i}-{j}")))
                        .collect(),
                )
            })
            .collect();
        let clamped = clamp_action_rows(rows);
        assert_eq!(clamped.len(), MAX_ACTION_ROWS);
        for row in &clamped {
            let CreateActionRow::Buttons(buttons) = row else {
                panic!("expected a button row");
            };
            assert!(buttons.len() <= MAX_ROW_COMPONENTS);
        }
    }

    #[test]
    fn pagination_none_for_single_page() {
        assert!(pagination_row("pfx", 0, 1).is_none());
        assert!(pagination_row("pfx", 0, 0).is_none());
        assert!(pagination_row("pfx", 0, 2).is_some());
    }
}
