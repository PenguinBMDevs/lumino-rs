//! 分割线手柄 — yinhe `widgets/split_handle.rs:79` 的 iced 迁移桩
//!
//! 原 `egui` 实现在 1px 线上扩展命中并绘制 hover/按下态；
//! iced 桩以 `container + mouse_area` 重建，状态由 Host 持有，样式走 `Theme`。

use iced_core::{Color, Length};
use iced_widget::{container, mouse_area};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 渲染水平分割手柄（横线，拖动改变上下比例）
pub fn horizontal<'a>(window: &'a Window, is_hovered: bool, is_active: bool) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let base = palette.background.strong.color;
    let hover = palette.background.strong.color.scale_alpha(0.8);
    let active = palette.primary.base.color;
    let color = if is_active {
        active
    } else if is_hovered {
        hover
    } else {
        base
    };
    let handle = container(iced_widget::Space::new().height(4).width(Length::Fill)).style(
        move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(color)),
            ..Default::default()
        },
    );

    mouse_area(handle)
        .on_press(lumino_ui_core::message::null())
        .interaction(iced_core::mouse::Interaction::ResizingVertically)
        .into()
}

/// 渲染垂直分割手柄（竖线）
pub fn vertical<'a>(window: &'a Window, is_hovered: bool, is_active: bool) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let base = palette.background.strong.color;
    let hover = palette.background.strong.color.scale_alpha(0.8);
    let active = palette.primary.base.color;
    let color = if is_active {
        active
    } else if is_hovered {
        hover
    } else {
        base
    };
    let handle = container(iced_widget::Space::new().width(4).height(Length::Fill)).style(
        move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(color)),
            ..Default::default()
        },
    );

    mouse_area(handle)
        .on_press(lumino_ui_core::message::null())
        .interaction(iced_core::mouse::Interaction::ResizingHorizontally)
        .into()
}
