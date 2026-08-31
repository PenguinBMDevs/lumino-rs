//! 图文混排 — yinhe `widgets/icon_text.rs:38` 的 iced 迁移桩

use iced_core::Alignment;
use iced_widget::{row, text};

use lumino_ui_core::resources::icon::{Icon, view_with_size_and_theme};
use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

pub fn view<'a>(window: &'a Window, icon: Icon, label: &'a str) -> Element<'a> {
    row![
        view_with_size_and_theme(icon, 14, 14, Some(&window.theme)),
        text(label).size(12),
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .into()
}
