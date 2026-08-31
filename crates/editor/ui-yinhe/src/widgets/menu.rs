//! 菜单项 — yinhe `widgets/menu.rs:18` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{button, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 渲染菜单项按钮（铺满整行，选中高亮，无边框）
pub fn menu_item_button<'a>(window: &'a Window, selected: bool, label: String) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = if selected {
        palette.background.strong.color
    } else {
        iced_core::Color::TRANSPARENT
    };
    let txt = if selected {
        palette.background.strong.text
    } else {
        palette.background.base.text
    };
    button(
        text(label)
            .size(12)
            .style(move |_t: &Theme| iced_widget::text::Style { color: Some(txt) }),
    )
    .on_press(lumino_ui_core::message::null())
    .width(Length::Fill)
    .padding([6, 10])
    .style(move |_t: &Theme, status| {
        let c = if selected {
            bg
        } else if status == button::Status::Hovered {
            palette.background.weak.color
        } else {
            iced_core::Color::TRANSPARENT
        };
        button::Style {
            background: Some(iced_core::Background::Color(c)),
            border: iced_core::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }
    })
    .into()
}
