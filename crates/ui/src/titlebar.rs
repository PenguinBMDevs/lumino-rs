mod logo;
mod menu;
mod traffic;

use iced_core::{Alignment, Length};
use iced_widget::{container, mouse_area, row};

use super::Element;
use crate::{Message, Theme, window};

pub struct Titlebar {}

impl Titlebar {
    pub fn new() -> Self {
        Self {}
    }

    pub fn view<'a>(&'a self, window: &'a window::Window) -> Element<'a> {
        let row = if cfg!(target_os = "macos") {
            row![]
        } else {
            row![logo::view(), menu::view(), traffic::view(window),]
        };

        let inner = container(row)
            .width(Length::Fill)
            .height(30)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style::default().background(if window.is_focused {
                    palette.background.neutral.color
                } else {
                    palette.background.weaker.color
                })
            })
            .align_y(Alignment::Start);

        mouse_area(inner)
            .on_press(Message::Core(lumino_core::event!(Window.Drag)))
            .on_double_click(Message::Core(lumino_core::event!(Window.ToggleMaximize)))
            .into()
    }
}
