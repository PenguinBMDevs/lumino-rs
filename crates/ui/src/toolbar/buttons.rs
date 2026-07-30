//! 工具栏按钮辅助函数
//!
//! 提供带悬浮提示（tooltip）的工具栏按钮工厂函数。

use crate::resources::icon;
use crate::toolbar::{Event, Tool};
use crate::widget;
use crate::{Element, Message, Theme, window};
use iced_widget::button;

/// 工具按钮
pub fn tool_button<'a>(
    icon_enum: icon::Icon,
    tooltip: &'a str,
    on_press: Message,
    window: &'a window::Window,
    on_hover_msg: Option<Message>,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let btn = button(icon::view_with_size_and_theme(
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
    .padding(4);

    let btn = apply_hover(btn, on_hover_msg);

    widget::with_tooltip_bottom(btn, tooltip).into()
}

/// 翻转按钮（有选中时可用，无选中时禁用）
pub fn flip_button<'a>(
    icon_enum: icon::Icon,
    tooltip: &'a str,
    on_press: Message,
    enabled: bool,
    window: &'a window::Window,
    on_hover_msg: Option<Message>,
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

    let btn = if enabled { btn.on_press(on_press) } else { btn };

    let btn = apply_hover(btn, on_hover_msg);

    widget::with_tooltip_bottom(btn, tooltip).into()
}

/// 工具选择器
pub fn tool_selector<'a>(
    icon_enum: icon::Icon,
    tooltip: &'a str,
    tool: Tool,
    current_tool: Tool,
    window: &'a window::Window,
    on_hover_msg: Option<Message>,
) -> Element<'a> {
    tool_selector_enabled(icon_enum, tooltip, tool, current_tool, true, window, on_hover_msg)
}

/// 带启用/禁用状态的工具选择器
pub fn tool_selector_enabled<'a>(
    icon_enum: icon::Icon,
    tooltip: &'a str,
    tool: Tool,
    current_tool: Tool,
    enabled: bool,
    window: &'a window::Window,
    on_hover_msg: Option<Message>,
) -> Element<'a> {
    let is_selected = tool == current_tool && enabled;
    let palette = window.theme.extended_palette();

    let mut btn = button(icon::view_with_size_and_theme(
        icon_enum,
        17,
        17,
        Some(&window.theme),
    ))
    .style(move |_theme: &Theme, status| {
        let bg = if !enabled {
            iced_core::Color::TRANSPARENT
        } else if is_selected {
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
    .padding(iced_core::Padding::new(4.0));

    // 仅在启用时绑定 on_press
    if enabled {
        btn = btn.on_press(Event::tool_selected(tool));
    }

    let btn = apply_hover(btn, on_hover_msg);

    widget::with_tooltip_bottom(btn, tooltip).into()
}

/// 为按钮挂接悬停事件：进入时发送按钮名，离开时发送 `None`。
///
/// iced 0.14 的 `Button` 本身不带 `on_hover`，需用 `MouseArea` 包裹。
/// 这样底部状态栏左侧即可在鼠标悬停工具栏按钮时显示对应描述文字。
fn apply_hover<'a>(
    btn: iced_widget::button::Button<'a, Message, Theme, iced_wgpu::Renderer>,
    on_hover_msg: Option<Message>,
) -> Element<'a> {
    match on_hover_msg {
        Some(msg) => iced_widget::mouse_area(btn)
            .on_enter(msg)
            .on_exit(Event::button_hovered(None))
            .into(),
        None => btn.into(),
    }
}
