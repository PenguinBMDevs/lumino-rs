//! 工具选择器

use iced_widget::{button, container, row, space};

use crate::resources::icon;
use crate::toolbar::{Event, RESIZE_HANDLE_HEIGHT, Tool};
use crate::{Element, Message, Theme, window};

use super::Toolbar;

pub(super) fn tools<'a>(toolbar: &'a Toolbar, window: &'a window::Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let content_height = toolbar.height - RESIZE_HANDLE_HEIGHT;

    container(
        row![
            tool_selector(
                icon::MousePointer,
                Tool::Pointer,
                toolbar.current_tool,
                window
            ),
            space().width(4),
            tool_selector(icon::Pencil, Tool::Pencil, toolbar.current_tool, window),
            space().width(4),
            tool_selector(icon::Eraser, Tool::Eraser, toolbar.current_tool, window),
        ]
        .align_y(iced_core::Alignment::Center),
    )
    .width(285)
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

pub(super) fn tool_selector<'a>(
    icon_enum: icon::Icon,
    tool: Tool,
    current_tool: Tool,
    window: &'a window::Window,
) -> Element<'a> {
    let is_selected = tool == current_tool;
    let palette = window.theme.extended_palette();

    button(icon::view_with_size_and_theme(
        icon_enum,
        17,
        17,
        Some(&window.theme),
    ))
    .on_press(Event::tool_selected(tool))
    .style(move |_theme: &Theme, status| {
        let bg = if is_selected {
            palette.background.strong.color
        } else if status == iced_widget::button::Status::Hovered {
            palette.background.weak.color
        } else {
            iced_core::Color::TRANSPARENT
        };

        button::Style {
            border: iced_core::Border {
                radius: 3.0.into(),
                width: 0.0,
                color: iced_core::Color::TRANSPARENT,
            },
            ..Default::default()
        }
        .with_background(bg)
    })
    .padding(iced_core::Padding::new(4.0))
    .into()
}
