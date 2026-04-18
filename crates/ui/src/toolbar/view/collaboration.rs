//! 协作按钮区域

use iced_core::Alignment;
use iced_widget::{button, container, row, space, text};

use crate::resources::icon;
use crate::toolbar::{Event, RESIZE_HANDLE_HEIGHT};
use crate::{Element, Message, Theme, window};

use super::Toolbar;

pub(super) fn collaboration_button<'a>(
    toolbar: &'a Toolbar,
    window: &'a window::Window,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let content_height = toolbar.height - RESIZE_HANDLE_HEIGHT;

    container(
        button(
            row![
                icon::view_with_size_and_theme(icon::Users, 18, 18, Some(&window.theme)),
                space().width(6),
                text("多人协作")
                    .size(14)
                    .color(palette.background.weakest.text),
            ]
            .align_y(Alignment::Center),
        )
        .on_press(Event::open_collaboration_dialog())
        .style(move |_theme: &Theme, status| {
            let bg = match status {
                iced_widget::button::Status::Hovered => palette.background.weak.color,
                _ => palette.background.weakest.color,
            };
            button::Style {
                border: iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                ..Default::default()
            }
            .with_background(bg)
        })
        .padding([8, 12]),
    )
    .height(content_height)
    .align_y(iced_core::alignment::Vertical::Center)
    .padding([0, 16])
    .style(move |_theme: &Theme| {
        container::Style::default().background(palette.background.weakest.color)
    })
    .into()
}
