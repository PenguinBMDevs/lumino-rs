//! 播放控制区域

use iced_core::Alignment;
use iced_widget::{button, container, row, space};

use crate::resources::icon;
use crate::toolbar::{Event, RESIZE_HANDLE_HEIGHT};
use crate::{Element, Message, Theme, window};

use super::Toolbar;

pub(super) fn playback_controls<'a>(
    toolbar: &'a Toolbar,
    window: &'a window::Window,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let content_height = toolbar.height - RESIZE_HANDLE_HEIGHT;

    container(
        row![
            tool_button(icon::SkipBackward, Event::skip_backward(), window),
            space().width(4),
            if toolbar.is_playing {
                tool_button(icon::Pause, Event::pause(), window)
            } else {
                tool_button(icon::Play, Event::play(), window)
            },
            space().width(4),
            tool_button(icon::SkipForward, Event::skip_forward(), window),
        ]
        .align_y(Alignment::Center),
    )
    .width(132)
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

pub(super) fn tool_button<'a>(
    icon_enum: icon::Icon,
    on_press: Message,
    window: &'a window::Window,
) -> Element<'a> {
    button(icon::view_with_size_and_theme(
        icon_enum,
        20,
        20,
        Some(&window.theme),
    ))
    .on_press(on_press)
    .style(|_theme: &Theme, _status| {
        button::Style::default().with_background(iced_core::Color::TRANSPARENT)
    })
    .padding(4)
    .into()
}
