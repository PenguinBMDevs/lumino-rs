//! 空状态提示 — yinhe `widgets/hint.rs:12` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{container, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

pub fn empty_hint<'a>(window: &'a Window, hint: &'a str) -> Element<'a> {
    let palette = window.theme.extended_palette();
    container(
        text(hint)
            .size(12)
            .style(move |_t: &Theme| iced_widget::text::Style {
                color: Some(palette.background.weak.text),
            }),
    )
    .padding([8, 0])
    .width(Length::Fill)
    .center_x(Length::Fill)
    .into()
}
