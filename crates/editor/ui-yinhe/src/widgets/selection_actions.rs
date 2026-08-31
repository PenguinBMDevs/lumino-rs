//! 选中动作 — yinhe `widgets/selection_actions.rs:267` 的 iced 迁移桩

use iced_widget::{button, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 渲染选中动作工具栏（转调/去重/删除等）
pub fn view<'a>(window: &'a Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    row![
        button(text("transpose_up").size(11))
            .on_press(lumino_ui_core::message::null())
            .padding([4, 8]),
        button(text("transpose_down").size(11))
            .on_press(lumino_ui_core::message::null())
            .padding([4, 8]),
        button(text("dedup_within").size(11))
            .on_press(lumino_ui_core::message::null())
            .padding([4, 8]),
        button(text("dedup_across").size(11))
            .on_press(lumino_ui_core::message::null())
            .padding([4, 8]),
        button(
            text("delete")
                .size(11)
                .style(move |_t: &Theme| iced_widget::text::Style {
                    color: Some(palette.danger.base.color),
                })
        )
        .on_press(lumino_ui_core::message::null())
        .padding([4, 8]),
    ]
    .spacing(6)
    .into()
}
