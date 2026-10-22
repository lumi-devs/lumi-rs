use serenity::all::Colour;

use lumi_ui::Card;
use lumi_ui::CardReply;

#[must_use]
pub fn log_card(color: u32, title: &str, lines: Vec<String>) -> CardReply {
    Card::lines(title, lines)
        .accent(Some(Colour::new(color)))
        .into_reply()
}
