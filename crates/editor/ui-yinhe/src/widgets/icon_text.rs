//! 图文混排 — yinhe `widgets/icon_text.rs:38` 的 iced 迁移桩

use iced_core::Alignment;
use iced_widget::{row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::Element;

pub fn view<'a>(_window: &'a Window, codepoint: char, label: &'a str) -> Element<'a> {
    row![
        crate::material_icons::icon(codepoint, 14.0, iced_core::Color::from_rgb(0.2, 0.2, 0.2)),
        text(label).size(12),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}

/// 兼容旧 SVG 调用（保留，yinhe 模式已切 Material，lumino 侧 SVG 仍可复用）
pub fn view_svg<'a>(
    window: &'a Window,
    icon: lumino_ui_core::resources::icon::Icon,
    label: &'a str,
) -> Element<'a> {
    use lumino_ui_core::resources::icon::view_with_size_and_theme;
    row![
        view_with_size_and_theme(icon, 14, 14, Some(&window.theme)),
        text(label).size(12),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}
