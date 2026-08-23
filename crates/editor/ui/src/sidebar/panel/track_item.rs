//! 侧边栏音轨行视图
//!
//! 渲染单个音轨行（图标/标签、重命名输入、静音按钮、选中与拖拽样式）。

use iced_core::{Alignment, Length, Padding};
use iced_widget::{button, column, container, mouse_area, row, space, text, text_input};

use crate::resources::icon::{self, Icon};
use crate::sidebar::{Event, Track};
use crate::window;
use crate::{Element, Theme};

use super::color;

/// 渲染单个音轨行
pub(super) fn view_track_item<'a>(
    track: &'a Track,
    is_selected: bool,
    window: &'a window::Window,
    renaming_track: Option<&'a (usize, String)>,
    is_dragging: bool,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let is_renaming = renaming_track.map(|(id, _)| *id) == Some(track.id);
    let track_color = track.color;
    let text_color = color::track_text_color(track_color, &window.theme);

    let left_icon: Element<'a> = if track.is_conductor {
        container(icon::view_with_size_and_theme(
            Icon::Clock,
            18,
            18,
            Some(&window.theme),
        ))
        .width(24)
        .align_x(iced_core::alignment::Horizontal::Left)
        .align_y(iced_core::alignment::Vertical::Center)
        .padding(iced_core::Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 2.0,
        })
        .into()
    } else {
        container(
            text(&track.display_label)
                .size(14)
                .font(iced_core::Font {
                    weight: iced_core::font::Weight::Bold,
                    ..Default::default()
                })
                .style(move |_theme: &Theme| text::Style {
                    color: Some(text_color),
                }),
        )
        .width(36)
        .align_x(iced_core::alignment::Horizontal::Left)
        .align_y(iced_core::alignment::Vertical::Center)
        .padding(iced_core::Padding {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 2.0,
        })
        .into()
    };

    let name: Element<'a> = if is_renaming {
        let buffer = renaming_track.map(|(_, buf)| buf.as_str()).unwrap_or("");
        text_input("音轨名称", buffer)
            .on_input(|value| Event::track_rename_changed(track.id, value))
            .on_submit(Event::track_rename_confirmed(track.id))
            .width(Length::Fill)
            .padding(Padding::ZERO)
            .into()
    } else {
        text(&track.name)
            .size(14)
            .width(Length::Fill)
            .style(move |_theme: &Theme| text::Style {
                color: Some(text_color),
            })
            .into()
    };

    let solo_btn = button(
        text("S")
            .size(14)
            .font(iced_core::Font {
                weight: iced_core::font::Weight::Bold,
                ..Default::default()
            })
            .style(move |_theme: &Theme| text::Style {
                color: Some(if track.is_soloed {
                    palette.warning.base.color
                } else {
                    text_color
                }),
            }),
    )
    .on_press(Event::track_solo_toggled(track.id))
    .style(|_theme: &Theme, _status| {
        button::Style::default().with_background(iced_core::Color::TRANSPARENT)
    })
    .padding(0);

    let track_row = row![left_icon, space().width(4), name, solo_btn,]
        .align_y(Alignment::Center)
        .padding(4);

    // 选中与静音/独奏交互不依赖 button 的 on_press：行级 mouse_area 按下时
    // 同时发出"选中 + 拖拽候选开始"（Batch）。保留 button 仅用于视觉样式。
    let track_button = button(track_row)
        .width(Length::Fill)
        .style(move |theme: &Theme, status| {
            let bg = color::track_button_background(track_color, is_selected, status, theme);
            let border = if is_dragging {
                iced_core::Border {
                    radius: 6.0.into(),
                    width: 1.5,
                    color: palette.primary.strong.color,
                }
            } else {
                iced_core::Border {
                    radius: 6.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                }
            };
            button::Style {
                text_color,
                border,
                ..Default::default()
            }
            .with_background(bg)
        });

    // 左键按下：选中音轨 + 注册拖拽排序候选（长按计时起点）。
    // 右键点击打开上下文菜单。
    // 注意：button 已去除 on_press（否则会捕获 press 事件阻断本层 mouse_area），
    // 选中与静音/独奏交互由行内各控件的 on_press 各自处理。
    let track_button_with_menu = mouse_area(track_button)
        .on_press(crate::Message::Batch(vec![
            Event::track_selected(track.id),
            Event::track_reorder_started(track.id),
        ]))
        .on_right_press(Event::track_context_menu_opened(track.id));

    column![track_button_with_menu].spacing(2).into()
}
