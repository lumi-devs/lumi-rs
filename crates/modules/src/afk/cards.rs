use serenity::all::{
    ButtonStyle, CreateActionRow, CreateButton, CreateComponent, SeparatorSpacingSize,
};

use lumi_ui::container::Container;
use lumi_ui::panels::{action_row_of, component_emoji, pagination_row};
use lumi_ui::{CardOptions, CardReply, make_list_card};

/// A compact card: bold title, divider, body, optional action rows.
///
/// The AFK surfaces mirror lumi's hand-built containers (`**title**` rather
/// than a `##` card heading), so they are assembled from [`Container`].
fn compact_card(title: &str, body: &str, action_rows: Vec<CreateActionRow<'static>>) -> CardReply {
    let mut container = Container::new()
        .text(format!("**{title}**"))
        .separator(true, SeparatorSpacingSize::Small)
        .text(body.to_string());
    for row in action_rows {
        container = container.action_row(row);
    }
    CardReply {
        components: vec![CreateComponent::Container(container.into_create())],
        ephemeral: false,
    }
}

/// Card shown when a message mentions an AFK member.
pub fn mention_card(name: &str, reason: &str, duration: &str) -> CardReply {
    compact_card(
        &format!("\u{1f4a4} {name} is AFK"),
        &format!("**Reason:** {reason}\n**AFK for:** {duration}"),
        Vec::new(),
    )
}

/// Card replying to a member whose AFK status was just cleared.
pub fn welcome_back_card(duration: &str, mentions: u64, custom_id: Option<String>) -> CardReply {
    let rows = match (mentions, custom_id) {
        (0, _) | (_, None) => Vec::new(),
        (count, Some(custom_id)) => {
            let mut button = CreateButton::new(custom_id)
                .label(format!("View Mentions ({count})"))
                .style(ButtonStyle::Secondary);
            if let Some(emoji) = component_emoji("\u{1f4ec}") {
                button = button.emoji(emoji);
            }
            vec![action_row_of(vec![button])]
        }
    };
    compact_card(
        "\u{1f44b} Welcome Back!",
        &format!("AFK removed.\nAFK for **{duration}**."),
        rows,
    )
}

pub fn status_card(title: &str, body: &str) -> CardReply {
    compact_card(title, body, Vec::new())
}

/// Paged list of everyone who mentioned an AFK member.
pub fn mentions_card(
    items: &[String],
    page: usize,
    total_pages: usize,
    custom_id_prefix: &str,
) -> CardReply {
    let options = CardOptions {
        action_rows: pagination_row(custom_id_prefix, page, total_pages)
            .into_iter()
            .collect(),
        separator_above_action_rows: true,
        ..CardOptions::default()
    };
    let mut card = make_list_card("\u{1f4ec} Recent Mentions", items, options);
    card.ephemeral = true;
    card
}
