//! 属性浮动面板 — yinhe `dialogs/prop_panels.rs:203` 的 iced 迁移桩
//!
//! 原 `egui` 实现为独立 viewport 承载音轨/工程属性；iced 桩以
//! `container + column + button + scrollable` 重建，独立窗口复用
//! `DialogManager`，内容复用 `crate::right_panel::info_panel` 与
//! `project_info` 的视图骨架（此处以占位文本示意）。

use iced_core::Length;
use iced_widget::{button, column, container, row, scrollable, text};

use lumino_ui_core::window::Window;
use lumino_ui_core::{Element, Theme};

/// 渲染音轨属性浮窗（占位，复用 info_panel 语义）
pub fn view_track_props<'a>(window: &'a Window, track_idx: u16) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;
    let weak = palette.background.weak.color;

    let header = row![
        button(text("dock_to_side").size(11))
            .on_press(lumino_ui_core::message::null())
            .padding([4, 8])
            .style(move |_t: &Theme, status| {
                let c = match status {
                    button::Status::Hovered => weak,
                    _ => iced_core::Color::TRANSPARENT,
                };
                button::Style {
                    background: Some(iced_core::Background::Color(c)),
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }),
        iced_widget::Space::new().width(Length::Fill),
        text(format!("track {track_idx}")).size(11),
    ];

    let body = scrollable(
        column![
            text(format!("track_props for track {track_idx}")).size(13),
            text("port / channel / color / mute / solo").size(11),
            text("info_panel::show_track_info 占位")
                .size(10)
                .style(move |_t: &Theme| iced_widget::text::Style {
                    color: Some(palette.background.weak.text),
                }),
        ]
        .spacing(8)
        .padding(8),
    )
    .height(Length::Fill);

    let content = column![header, iced_widget::Space::new().height(4), body]
        .spacing(0)
        .padding(10);

    container(content)
        .width(Length::Fixed(380.0))
        .height(Length::Fixed(520.0))
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

/// 渲染工程属性浮窗
pub fn view_project_props<'a>(window: &'a Window) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let bg = palette.background.base.color;
    let weak = palette.background.weak.color;

    let header = row![
        button(text("dock_to_side").size(11))
            .on_press(lumino_ui_core::message::null())
            .padding([4, 8])
            .style(move |_t: &Theme, status| {
                let c = match status {
                    button::Status::Hovered => weak,
                    _ => iced_core::Color::TRANSPARENT,
                };
                button::Style {
                    background: Some(iced_core::Background::Color(c)),
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }
            }),
        iced_widget::Space::new().width(Length::Fill),
        text("project").size(11),
    ];

    let body = scrollable(
        column![
            text("project_props").size(13),
            text("title / ppq / time_sig / key_sig").size(11),
            text("project_info::show 占位")
                .size(10)
                .style(move |_t: &Theme| iced_widget::text::Style {
                    color: Some(palette.background.weak.text),
                }),
        ]
        .spacing(8)
        .padding(8),
    )
    .height(Length::Fill);

    let content = column![header, iced_widget::Space::new().height(4), body]
        .spacing(0)
        .padding(10);

    container(content)
        .width(Length::Fixed(420.0))
        .height(Length::Fixed(520.0))
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
