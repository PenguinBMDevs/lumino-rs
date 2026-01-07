use iced_core::Length;
use iced_widget::{container, space};

use super::Element;

pub struct Editor {

}

impl Editor {
    pub fn new() -> Self {
        Self {

        }
    }

    pub fn view(&self) -> Element<'_> {
        container(space())
            .width(Length::Fill)
            .into()
    }
}
