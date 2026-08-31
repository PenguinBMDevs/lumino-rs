//! 悬浮高亮 — yinhe `widgets/hover.rs:73` 的 iced 迁移桩
//!
//! 原 `egui` 实现在列表行 hover 时以 3% 提亮；iced 桩以 `mouse_area`
//! + `container` 背景切换实现。

use iced_widget::{container, mouse_area};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

pub fn hover_highlight<'a>(window: &'a Window, content: Element<'a>, hovered: bool) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = if hovered {
        palette.background.weak.color.scale_alpha(0.35)
    } else {
        iced_core::Color::TRANSPARENT
    };
    let inner = container(content).style(move |_t: &Theme| container::Style {
        background: Some(iced_core::Background::Color(bg)),
        border: iced_core::Border {
            radius: 3.0.into(),
            ..Default::default()
        },
        ..Default::default()
    });
    mouse_area(inner)
        .on_press(lumino_ui_core::message::null())
        .into()
}
