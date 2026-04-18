//! 调整大小手柄区域

use iced_widget::{container, mouse_area, space};

use crate::toolbar::{Event, RESIZE_HANDLE_HEIGHT};
use crate::{Element, Message, Theme, window};

use super::Toolbar;

pub(super) fn resize_handle<'a>(toolbar: &'a Toolbar, window: &'a window::Window) -> Element<'a> {
    let palette = window.theme.extended_palette();

    mouse_area(
        container(space().height(iced_widget::core::Length::Fixed(RESIZE_HANDLE_HEIGHT)))
            .width(iced_widget::core::Length::Fill)
            .style(move |_theme: &Theme| {
                container::Style::default().background(if toolbar.is_resizing {
                    palette.primary.strong.color
                } else {
                    palette.background.weakest.color
                })
            }),
    )
    .interaction(iced_core::mouse::Interaction::ResizingVertically)
    .on_press(Message::Toolbar(Event::ResizeDragStarted(
        iced_core::Point::new(0.0, 0.0),
    )))
    .on_release(Message::Toolbar(Event::ResizeDragEnded))
    .into()
}
