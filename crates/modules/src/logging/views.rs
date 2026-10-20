use serenity::all::Colour;

use lumi_ui::Card;
use lumi_ui::CardReply;

/// A log entry card: an accent colour, a title and body lines.
pub fn log_card(color: u32, title: &str, lines: &[String]) -> CardReply {
    Card::lines(title, lines.to_vec())
        .accent(Some(Colour::new(color)))
        .into_reply()
}
