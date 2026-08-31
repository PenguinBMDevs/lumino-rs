//! 数值输入 — yinhe `widgets/numeric_input.rs:37` 的 iced 迁移桩
//!
//! 原 `egui::DragValue` 的中文句号折算；iced 桩以 `text_input` 重建，
//! 输入时把 `。` 替换为 `.` 后解析。

use iced_widget::{container, text_input};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 中文句号折算 parser（与 yinhe `decimal_parser` 等价）
pub fn decimal_parser(s: &str) -> Option<f64> {
    let normalized = s.replace('。', ".");
    let filtered: String = normalized
        .chars()
        .filter(|c| {
            *c == '-' || *c == '+' || *c == '.' || *c == 'e' || *c == 'E' || c.is_ascii_digit()
        })
        .collect();
    filtered.parse().ok()
}

/// 渲染数值输入框
pub fn view<'a>(window: &'a Window, value: String, placeholder: &'a str) -> Element<'a> {
    let palette = window.theme.extended_palette();
    container(
        text_input(placeholder, &value)
            .on_input(|_| lumino_ui_core::message::null())
            .padding(6)
            .size(12),
    )
    .padding(1)
    .style(move |_t: &Theme| container::Style {
        background: Some(iced_core::Background::Color(palette.background.weak.color)),
        border: iced_core::Border {
            radius: 4.0.into(),
            width: 1.0,
            color: palette.background.strong.color,
        },
        ..Default::default()
    })
    .into()
}
