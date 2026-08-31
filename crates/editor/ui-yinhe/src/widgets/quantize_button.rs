//! 量化按钮 — yinhe `widgets/quantize_button.rs:69` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{button, container, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 渲染左上角量化按钮（与 `quantize_popup` 联动）
pub fn view<'a>(window: &'a Window, label: &'a str, hovered: bool) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = if hovered {
        palette.background.weak.color.scale_alpha(0.6)
    } else {
        iced_core::Color::TRANSPARENT
    };
    let txt = if hovered {
        palette.background.base.text
    } else {
        palette.background.weak.text
    };

    container(
        button(
            text(label)
                .size(11)
                .style(move |_t: &Theme| iced_widget::text::Style { color: Some(txt) }),
        )
        .on_press(lumino_ui_core::message::null())
        .padding([4, 6])
        .style(move |_t: &Theme, _| button::Style {
            background: Some(iced_core::Background::Color(bg)),
            border: iced_core::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        }),
    )
    .width(Length::Fixed(20.0))
    .height(Length::Fixed(20.0))
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .style(move |_t: &Theme| container::Style {
        background: Some(iced_core::Background::Color(
            palette.background.weakest.color,
        )),
        ..Default::default()
    })
    .into()
}
