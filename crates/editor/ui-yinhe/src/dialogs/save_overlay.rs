//! 保存遮罩 — yinhe `dialogs/save_overlay.rs:68` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{column, container, progress_bar, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

pub fn view<'a>(window: &'a Window, progress: f32) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;

    let content = column![
        text("saving...").size(13),
        progress_bar(0.0..=1.0, progress),
    ]
    .spacing(10)
    .padding(16);

    container(content)
        .width(Length::Fixed(320.0))
        .height(Length::Fixed(120.0))
        .style(move |_t: &Theme| container::Style {
            background: Some(iced_core::Background::Color(bg)),
            border: iced_core::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
