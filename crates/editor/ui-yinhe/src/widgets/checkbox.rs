//! 复选框 — yinhe `widgets/checkbox.rs:30` 的 iced 迁移桩

use iced_widget::{checkbox, container, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 渲染主题化复选框
pub fn view<'a>(window: &'a Window, checked: bool, label: &'a str) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let check_color = palette.background.base.text;
    checkbox(checked)
        .label(label)
        .on_toggle(|_| lumino_ui_core::message::null())
        .style(move |_t: &Theme, _status| iced_widget::checkbox::Style {
            background: iced_core::Background::Color(palette.background.weak.color),
            icon_color: check_color,
            border: iced_core::Border {
                radius: 3.0.into(),
                width: 1.0,
                color: palette.background.strong.color,
            },
            text_color: Some(palette.background.base.text),
        })
        .into()
}
