use iced_core::{Length};
use iced_widget::{container, space};

use super::Element;
use crate::{
    Theme,
};

pub struct StatusBar {

}

impl StatusBar {
    pub fn new() -> Self {
        Self {

        }
    }

    pub fn view<'a>(&'a self) -> Element<'a> {
        container(space())
            .width(Length::Fill)
            .height(20)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style::default()
                    .background(palette.background.weak.color)
            })
            .into()
    }
}
