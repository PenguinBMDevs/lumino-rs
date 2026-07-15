//! 设置面板样式工厂

use lumino_ui_core::Theme;
use iced_core::Border;
use iced_widget::{button, container, text};

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
