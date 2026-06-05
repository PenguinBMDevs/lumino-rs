//! 工具栏按钮辅助函数

use crate::resources::icon;
use crate::toolbar::{Event, Tool};
use crate::{Element, Message, Theme, window};
use iced_widget::button;

/// 工具按钮
pub fn tool_button<'a>(
    icon_enum: icon::Icon,
    on_press: Message,
    window: &'a window::Window,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    button(icon::view_with_size_and_theme(
        icon_enum,
        20,
        20,
        Some(&window.theme),
    ))
    .on_press(on_press)
    .style(move |_theme: &Theme, status| {
        let bg = if status == iced_widget::button::Status::Hovered {
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
    .padding(4)
    .into()
}

/// 翻转按钮（有选中时可用，无选中时禁用）
pub fn flip_button<'a>(
    icon_enum: icon::Icon,
    on_press: Message,
    enabled: bool,
    window: &'a window::Window,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let btn = button(icon::view_with_size_and_theme(
        icon_enum,
        20,
        20,
        Some(&window.theme),
    ))
    .style(move |_theme: &Theme, status| {
        let bg = if !enabled {
            iced_core::Color::TRANSPARENT
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
    .padding(4);

    if enabled {
        btn.on_press(on_press).into()
    } else {
        btn.into()
    }
}

/// 工具选择器
pub fn tool_selector<'a>(
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
