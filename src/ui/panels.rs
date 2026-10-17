use serenity::all::{CreateActionRow, CreateButton, ReactionType};

/// Emoji string to a button/option emoji, mirroring `componentEmoji` in `Panels.ts`.
#[must_use]
pub fn component_emoji(name: &str) -> Option<ReactionType> {
    ReactionType::try_from(name).ok()
}

/// An action row of buttons.
pub fn action_row_of(buttons: Vec<CreateButton<'static>>) -> CreateActionRow<'static> {
    CreateActionRow::buttons(buttons)
}
