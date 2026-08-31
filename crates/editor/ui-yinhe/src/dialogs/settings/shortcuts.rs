//! 快捷键设置页 — yinhe `dialogs/settings/shortcuts.rs:247` 的 iced 迁移桩

use iced_core::Length;
use iced_widget::{button, column, container, row, scrollable, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

struct ShortcutRow {
    action: &'static str,
    key: &'static str,
}

const ROWS: &[ShortcutRow] = &[
    ShortcutRow {
        action: "save",
        key: "Ctrl+S",
    },
    ShortcutRow {
        action: "undo",
        key: "Ctrl+Z",
    },
    ShortcutRow {
        action: "redo",
        key: "Ctrl+Y",
    },
    ShortcutRow {
        action: "play",
        key: "Space",
    },
];

pub fn view<'a>(window: &'a Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;
    let weak = palette.background.weak.color;

    let header = row![
        text("action").size(11).width(Length::Fixed(150.0)),
        text("shortcut").size(11),
        iced_widget::Space::new().width(Length::Fill),
        text("edit").size(11),
    ]
    .spacing(8);

    let rows: Vec<Element<'a>> = ROWS
        .iter()
        .map(|r| {
            row![
                text(r.action).size(12).width(Length::Fixed(150.0)),
                container(text(r.key).size(12))
                    .padding([4, 10])
                    .style(move |_t: &Theme| container::Style {
                        background: Some(iced_core::Background::Color(weak)),
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
                iced_widget::Space::new().width(Length::Fill),
                button(text("record").size(11))
                    .on_press(lumino_ui_core::message::null())
                    .padding([4, 8]),
                button(text("clear").size(11))
                    .on_press(lumino_ui_core::message::null())
                    .padding([4, 8]),
            ]
            .spacing(8)
            .into()
        })
        .collect();

    let content = column![
        text("shortcuts").size(14),
        header,
        scrollable(column(rows).spacing(6)).height(Length::Fixed(400.0))
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
