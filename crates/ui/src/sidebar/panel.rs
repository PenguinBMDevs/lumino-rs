use iced_core::{Length};
use iced_widget::{container, space};

use crate::{
    Element,
    Theme,
};

pub fn view<'a>() -> Element<'a> {
    container(space())
        .width(200)
        .height(Length::Fill)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default()
                .background(palette.background.weakest.color)
        })
        .into()
}
