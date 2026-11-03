//! 工具栏按钮辅助函数
//!
//! 提供带悬浮提示（tooltip）的工具栏按钮工厂函数。
//!
//! # 尺寸契约（供 `overflow` 的宽度预算使用）
//!
//! 工具栏折叠是按**预估宽度**决策的（渲染前拿不到真实布局），所以按钮的几何必须
//! 有唯一来源：工厂函数只从下面的常量取图标尺寸与内边距，`overflow` 只从
//! [`TOOL_SELECTOR_WIDTH`] / [`TOOL_BUTTON_WIDTH`] 取按钮宽度。
//! **改按钮内边距或图标尺寸 = 同时改预算**，否则会出现"要么白折叠、要么溢出被裁"。

use crate::resources::icon;
use crate::toolbar::{Event, Tool};
use crate::widget;
use crate::{Element, Message, Theme, window};
use iced_widget::button;

/// 工具选择器图标边长（`tool_selector` / `tool_selector_custom`）
const TOOL_SELECTOR_ICON: u32 = 17;
/// 普通工具按钮图标边长（`tool_button` / `flip_button`）
const TOOL_BUTTON_ICON: u32 = 20;
/// 以上两类按钮的统一内边距（四边相同）
const TOOL_BUTTON_PADDING: u16 = 4;

/// 工具选择器按钮的固定宽度（图标 + 左右内边距）
///
/// 预算口径：`tool_selector` 的宽度**必须**等于本值。
pub const TOOL_SELECTOR_WIDTH: f32 = TOOL_SELECTOR_ICON as f32 + (TOOL_BUTTON_PADDING as f32) * 2.0;

/// 普通工具按钮的固定宽度（图标 + 左右内边距）
///
/// 预算口径：`tool_button` / `flip_button` 的宽度**必须**等于本值；
/// 「更多」按钮也用 `tool_button`，故其宽度同源。
pub const TOOL_BUTTON_WIDTH: f32 = TOOL_BUTTON_ICON as f32 + (TOOL_BUTTON_PADDING as f32) * 2.0;

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
        TOOL_BUTTON_ICON,
        TOOL_BUTTON_ICON,
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
    .padding(TOOL_BUTTON_PADDING);

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
        TOOL_BUTTON_ICON,
        TOOL_BUTTON_ICON,
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
    .padding(TOOL_BUTTON_PADDING);

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
    tool_selector_custom(
        icon_enum,
        tooltip,
        tool == current_tool,
        Event::tool_selected(tool),
        window,
        on_hover_msg,
    )
}

/// 工具选择器（自定义图标/选中态/点击消息）
///
/// 与 [`tool_selector`] 视觉完全一致（图标 17px、内边距 4、选中高亮 `background.strong`），
/// 但允许传入任意图标与 `on_press`，用于"曲线工具"按钮跟随当前激活的绘制子工具切换图标、
/// 并以统一的点击消息（普通点击=选择曲线工具，Ctrl+点击=打开画刷下拉）驱动交互。
pub fn tool_selector_custom<'a>(
    icon_enum: icon::Icon,
    tooltip: &'a str,
    is_selected: bool,
    on_press: Message,
    window: &'a window::Window,
    on_hover_msg: Option<Message>,
) -> Element<'a> {
    let palette = window.theme.extended_palette();

    let btn = button(icon::view_with_size_and_theme(
        icon_enum,
        TOOL_SELECTOR_ICON,
        TOOL_SELECTOR_ICON,
        Some(&window.theme),
    ))
    .on_press(on_press)
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
    .padding(iced_core::Padding::new(TOOL_BUTTON_PADDING as f32));

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
