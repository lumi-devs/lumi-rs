use serenity::all::Colour;

use lumi_ui::Card;
use lumi_ui::CardReply;

pub fn log_card(color: u32, title: &str, lines: &[String]) -> CardReply {
    Card::lines(title, lines.to_vec())
        .accent(Some(Colour::new(color)))
        .into_reply()
}
