//! 撤销/重做按钮区域

use iced_core::Alignment;
use iced_widget::{container, row, space};

use crate::resources::icon;
use crate::toolbar::{Event, RESIZE_HANDLE_HEIGHT};
use crate::{Element, Theme, window};

use super::Toolbar;

pub(super) fn undo_redo_controls<'a>(
    toolbar: &'a Toolbar,
    window: &'a window::Window,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let content_height = toolbar.height - RESIZE_HANDLE_HEIGHT;

    container(
        row![
            super::playback::tool_button(icon::Undo, Event::undo(), window),
            space().width(4),
            super::playback::tool_button(icon::Redo, Event::redo(), window),
        ]
        .align_y(Alignment::Center),
    )
    .width(64)
    .height(content_height)
    .align_y(iced_core::alignment::Vertical::Center)
    .align_x(iced_core::alignment::Horizontal::Center)
    .style(move |_theme: &Theme| {
        container::Style::default()
            .background(palette.background.weak.color)
            .border(iced_core::Border {
                radius: 4.0.into(),
                width: 0.0,
                color: iced_core::Color::TRANSPARENT,
            })
    })
    .into()
}
