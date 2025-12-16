
use iced::{Element, Length, widget::{Container, Row, Text}};

use super::{
    message::Message,
};

const TOOLS: &[&'static str] = &[
    "Files",
    "Edit",
    "View",
    "Help"
];

pub fn view<'a>(

) -> Element<'a, Message> {

    let inner = TOOLS.iter()
        .fold(
            Row::new().spacing(16),
            |row, cfg| {
                let tab = Text::new(*cfg).size(14);
                row.push(tab)
            }
        );

    Container::new(inner)
        .width(Length::Fill)
        .padding([0, 8])
        .height(22)
        .into()
}
