//! 拖动排序 — yinhe `widgets/reorder.rs:194` 的 iced 迁移桩
//!
//! 原 `egui` 实现以 `DragDrop` + 自动滚动；iced 桩以 `column + mouse_area`
//! 重建可拖动列表，实际重排由 `Message` 在 Host 层处理。

use iced_core::Length;
use iced_widget::{column, container, mouse_area, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 可排序项
pub struct ReorderItem<'a> {
    pub label: String,
    pub content: Element<'a>,
}

/// 渲染可拖动排序列表
pub fn view<'a>(window: &'a Window, items: Vec<ReorderItem<'a>>) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let rows: Vec<Element<'a>> = items
        .into_iter()
        .enumerate()
        .map(|(idx, it)| {
            let row = container(
                column![
                    text(format!("{} {}", idx + 1, it.label)).size(12),
                    it.content
                ]
                .spacing(4)
                .padding(8),
            )
            .width(Length::Fill)
            .style(move |_t: &Theme| container::Style {
                background: Some(iced_core::Background::Color(
                    palette.background.weak.color.scale_alpha(0.3),
                )),
                border: iced_core::Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: palette.background.weak.color,
                },
                ..Default::default()
            });
            mouse_area(row)
                .on_press(lumino_ui_core::message::null())
                .interaction(iced_core::mouse::Interaction::Grab)
                .into()
        })
        .collect();

    container(column(rows).spacing(4).padding(4))
        .width(Length::Fill)
        .into()
}
