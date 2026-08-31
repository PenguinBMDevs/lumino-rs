//! 通用设置页 — yinhe `dialogs/settings/general.rs:126` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{checkbox, column, container, pick_list, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

pub fn view<'a>(window: &'a Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let behaviors = ["ReplaceTarget", "DeleteOriginal", "KeepOriginal"];
    let modes = ["Off", "DoubleClick", "RightClick", "Both"];

    let content = column![
        text("general").size(14),
        text("editing").size(12).style(move |_t: &Theme| {
            iced_widget::text::Style {
                color: Some(palette.primary.base.color),
            }
        }),
        row![
            text("allow_overlap").size(12).width(Length::Fixed(140.0)),
            checkbox(true)
                .label("allow")
                .on_toggle(|_| lumino_ui_core::message::null()),
        ]
        .spacing(8),
        row![
            text("blocked_behavior")
                .size(12)
                .width(Length::Fixed(140.0)),
            pick_list(behaviors.to_vec(), Some("ReplaceTarget"), |_| {
                lumino_ui_core::message::null()
            })
            .padding(6),
        ]
        .spacing(8),
        row![
            text("quick_delete").size(12).width(Length::Fixed(140.0)),
            pick_list(modes.to_vec(), Some("Off"), |_| {
                lumino_ui_core::message::null()
            })
            .padding(6),
        ]
        .spacing(8),
    ]
    .spacing(10)
    .padding(12);

    container(content)
        .width(Length::Fill)
        .style(move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            ..Default::default()
        })
        .into()
}
