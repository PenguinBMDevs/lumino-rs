//! 语言设置页 — yinhe `dialogs/settings/language.rs:78` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{button, column, container, row, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

const LANGUAGES: [&str; 4] = ["中文", "English", "日本語", "한국어"];

pub fn view<'a>(window: &'a Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;
    let weak = palette.background.weak.color;
    let strong = palette.background.strong.color;

    let rows: Vec<Element<'a>> = LANGUAGES
        .iter()
        .map(|lang| {
            let is_selected = *lang == "中文";
            let bg_col = if is_selected {
                strong
            } else {
                iced_core::Color::TRANSPARENT
            };
            let txt_col = if is_selected {
                palette.background.strong.text
            } else {
                palette.background.base.text
            };
            button(
                text(*lang)
                    .size(12)
                    .style(move |_t: &Theme| iced_widget::text::Style {
                        color: Some(txt_col),
                    }),
            )
            .on_press(lumino_ui_core::message::null())
            .width(Length::Fill)
            .padding([6, 10])
            .style(move |_t: &Theme, _| button::Style {
                background: Some(iced_core::Background::Color(bg_col)),
                border: iced_core::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
        })
        .collect();

    let content = column![
        text("language").size(14),
        column(rows).spacing(4),
        row![
            container(text("preview").size(11))
                .padding(8)
                .style(move |_t: &Theme| container::Style {
                    background: Some(iced_core::Background::Color(weak.scale_alpha(0.5))),
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
        ],
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
