//! 自定义 Widget 辅助函数
//!
//! 提供悬浮提示（tooltip）等通用 UI 能力的封装。

use crate::{Element, Message, Theme};
use iced_core::Background;
use iced_widget::tooltip::{self, Tooltip};

/// 为任意 widget 添加悬浮提示（tooltip）。
///
/// 默认使用 `Bottom` 位置，适用于工具栏等上方控件。
/// 可通过 `with_tooltip_position` 指定位置。
///
/// # 示例
/// ```ignore
/// use crate::widget;
/// let btn = tool_button(icon, Event::play(), window);
/// widget::with_tooltip(btn, "播放", tooltip::Position::Bottom)
///     .gap(4)
///     .into()
/// ```
pub fn with_tooltip<'a>(
    content: impl Into<Element<'a>>,
    tooltip_text: &'a str,
    position: tooltip::Position,
) -> Tooltip<'a, Message, Theme, iced_wgpu::Renderer> {
    Tooltip::new(content, iced_widget::text(tooltip_text).size(12), position)
        .gap(4)
        .padding(6)
        .style(tooltip_style)
        .snap_within_viewport(true)
}

/// 带默认位置（Bottom）的便捷版
pub fn with_tooltip_bottom<'a>(
    content: impl Into<Element<'a>>,
    tooltip_text: &'a str,
) -> Tooltip<'a, Message, Theme, iced_wgpu::Renderer> {
    with_tooltip(content, tooltip_text, tooltip::Position::Bottom)
}

/// Tooltip 统一样式：深色背景 + 浅色文字
fn tooltip_style(theme: &Theme) -> iced_widget::container::Style {
    let palette = theme.extended_palette();

    iced_widget::container::Style {
        background: Some(Background::Color(palette.background.base.color)),
        text_color: Some(palette.background.base.text),
        border: iced_core::Border {
            radius: 4.0.into(),
            width: 0.0,
            color: iced_core::Color::TRANSPARENT,
        },
        ..Default::default()
    }
}
