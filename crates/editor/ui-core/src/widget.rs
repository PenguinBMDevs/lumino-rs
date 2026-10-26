//! 自定义 Widget 辅助函数
//!
//! 提供悬浮提示（tooltip）等通用 UI 能力的封装。

use crate::{Element, Message, Renderer, Theme};
use iced_core::Background;
use iced_widget::tooltip::{self, Tooltip};

/// 数字框提交：解析整数表达式并检查闭区间。
///
/// 与曲线公式共用 `lumino-core` 的表达式解析器；这里只负责把错误转成可直接
/// 显示给用户的文本。失败时调用方必须保留原输入，不得静默改成 0。
pub fn eval_int_field(src: &str, min: i64, max: i64) -> Result<i64, String> {
    let value = lumino_core::shape::expr::eval_int(src).map_err(|e| e.to_string())?;
    if !(min..=max).contains(&value) {
        return Err(format!("数值 {value} 超出允许范围 {min}..={max}"));
    }
    Ok(value)
}

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
) -> Tooltip<'a, Message, Theme, Renderer> {
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
) -> Tooltip<'a, Message, Theme, Renderer> {
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

#[cfg(test)]
mod tests {
    use super::eval_int_field;

    #[test]
    fn test_eval_int_field_accepts_expression_and_range() {
        assert_eq!(eval_int_field("960*4", 1, 10_000), Ok(3840));
        assert_eq!(eval_int_field("24", 24, 32767), Ok(24));
    }

    #[test]
    fn test_eval_int_field_rejects_out_of_range_and_invalid() {
        assert!(eval_int_field("8", 24, 32767).is_err());
        assert!(eval_int_field("1.5", 0, 10).is_err());
        assert!(eval_int_field("1/0", 0, 10).is_err());
    }
}
