//! 设置面板样式工厂

use iced_core::Border;
use iced_widget::{button, container, text};
use lumino_ui_core::Theme;

/// 创建文本样式
pub fn text_style(
    color_fn: fn(&Theme) -> Option<iced_core::Color>,
) -> impl Fn(&Theme) -> text::Style + 'static {
    move |theme: &Theme| text::Style {
        color: color_fn(theme),
    }
}

/// 创建容器样式
pub fn container_style(
    background_fn: fn(&Theme) -> Option<iced_core::Background>,
    border_fn: fn(&Theme) -> Border,
    shadow_fn: fn(&Theme) -> iced_core::Shadow,
    text_color_fn: fn(&Theme) -> Option<iced_core::Color>,
) -> impl Fn(&Theme) -> container::Style + 'static {
    move |theme: &Theme| container::Style {
        background: background_fn(theme),
        border: border_fn(theme),
        shadow: shadow_fn(theme),
        text_color: text_color_fn(theme),
        snap: false,
    }
}

/// 创建按钮样式
pub fn button_style(
    background_fn: fn(&Theme, button::Status) -> Option<iced_core::Background>,
    border_fn: fn(&Theme) -> Border,
    text_color_fn: fn(&Theme) -> iced_core::Color,
) -> impl Fn(&Theme, button::Status) -> button::Style + 'static {
    move |theme: &Theme, status| button::Style {
        background: background_fn(theme, status),
        border: border_fn(theme),
        text_color: text_color_fn(theme),
        shadow: iced_core::Shadow::default(),
        snap: false,
    }
}

/// 创建内容文本样式
pub fn create_content_text_style() -> impl Fn(&Theme) -> text::Style + 'static {
    text_style(|theme| {
        let palette = theme.extended_palette();
        Some(palette.background.base.text)
    })
}

/// 创建占位符文本样式
pub fn create_placeholder_text_style() -> impl Fn(&Theme) -> text::Style + 'static {
    text_style(|theme| {
        let palette = theme.extended_palette();
        Some(palette.background.weak.text)
    })
}

/// 创建回声洞彩蛋文本样式（UI-006）
///
/// 底色取占位灰（与正文区分），`opacity` 用于「闪烁退出」动效。
///
/// 视觉闪烁用 **alpha** 而非「隐藏控件」表达：隐藏会把行高/宽度交回布局重算，
/// 闪烁期间整页会上下呼吸；alpha=0 保留布局盒，**零重排**。
pub fn create_echo_text_style(opacity: f32) -> impl Fn(&Theme) -> text::Style + 'static {
    let opacity = opacity.clamp(0.0, 1.0);
    move |theme: &Theme| {
        let mut color = theme.extended_palette().background.weak.text;
        color.a *= opacity;
        text::Style { color: Some(color) }
    }
}
