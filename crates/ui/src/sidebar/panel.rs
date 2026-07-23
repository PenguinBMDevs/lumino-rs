use iced_core::{Alignment, Length, Padding};
use iced_widget::{
    Stack, button, column, container, mouse_area, row, scrollable, space, text, text_input,
};
use lumino_core::i18n::{Language, main_translations};

use crate::{
    Element, Theme,
    resources::icon::{self, Icon},
    sidebar::{Event, RESIZE_HANDLE_WIDTH, Route, Track},
    window,
};

mod color;

/// 侧边栏视图参数
#[derive(Clone)]
pub struct SidebarViewParams<'a> {
    pub route: Route,
    pub tracks: &'a [Track],
    pub selected_track: usize,
    pub panel_width: f32,
    pub is_resizing: bool,
    pub context_menu_target_id: Option<usize>,
    pub renaming_track: Option<&'a (usize, String)>,
    pub color_picking_track: Option<usize>,
}

pub fn view<'a>(
    params: SidebarViewParams<'a>,
    window: &'a window::Window,
    language: Language,
) -> Element<'a> {
    let t = main_translations(language);
    let palette = window.theme.extended_palette();

    let content: Element<'a> = match params.route {
        Route::Arrangement => {
            // 音轨总览模式下仅显示添加音轨按钮，不显示音轨列表
            let mut col = column![].spacing(0).padding(8);

            // 添加音轨按钮
            let add_track_row = row![
                container(icon::view_with_size_and_theme(
                    Icon::Plus,
                    18,
                    18,
                    Some(&window.theme),
                ))
                .width(24)
                .align_x(iced_core::alignment::Horizontal::Left)
                .align_y(iced_core::alignment::Vertical::Center)
                .padding(Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 2.0,
                }),
                space().width(4),
                text(t.sidebar_add_track).size(14).width(Length::Fill),
            ]
            .align_y(Alignment::Center)
            .padding(6);

            let add_track_container = button(add_track_row)
                .width(Length::Fill)
                .on_press(Event::add_track())
                .style(|theme: &Theme, status| {
                    let palette = theme.extended_palette();
                    let bg = if status == iced_widget::button::Status::Hovered {
                        palette.background.weak.color
                    } else {
                        palette.background.base.color
                    };

                    button::Style {
                        text_color: palette.background.base.text,
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            width: 0.0,
                            color: iced_core::Color::TRANSPARENT,
                        },
                        ..Default::default()
                    }
                    .with_background(bg)
                });

            col = col.push(add_track_container);
            container(col).into()
        }
        Route::EventList => {
            // 事件列表面板占位（功能实施中）
            let placeholder = column![
                text("事件列表").size(14).style(|theme: &Theme| {
                    let palette = theme.extended_palette();
                    text::Style {
                        color: Some(palette.background.base.text),
                    }
                }),
                text("🚧 实施中...").size(12).style(|theme: &Theme| {
                    let palette = theme.extended_palette();
                    text::Style {
                        color: Some(palette.background.strong.text),
                    }
                }),
            ]
            .spacing(8)
            .align_x(Alignment::Center);

            container(placeholder)
                .width(Length::Fill)
                .height(Length::Fill)
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into()
        }
        Route::File => {
            // 全量渲染所有音轨——由 iced scrollable 原生处理滚动。
            let mut col = column![].spacing(0).padding(8);
            col = col.push(text(t.sidebar_track_list).size(12).style(|theme: &Theme| {
                let palette = theme.extended_palette();
                text::Style {
                    color: Some(palette.background.base.text),
                }
            }));

            for track in params.tracks {
                let track_container = view_track_item(
                    track,
                    track.id == params.selected_track,
                    window,
                    params.renaming_track,
                );
                col = col.push(track_container);
            }

            // 添加音轨按钮
            let add_track_row = row![
                container(icon::view_with_size_and_theme(
                    Icon::Plus,
                    18,
                    18,
                    Some(&window.theme),
                ))
                .width(24)
                .align_x(iced_core::alignment::Horizontal::Left)
                .align_y(iced_core::alignment::Vertical::Center)
                .padding(Padding {
                    top: 0.0,
                    right: 0.0,
                    bottom: 0.0,
                    left: 2.0,
                }),
                space().width(4),
                text(t.sidebar_add_track).size(14).width(Length::Fill),
            ]
            .align_y(Alignment::Center)
            .padding(6);

            let add_track_container = button(add_track_row)
                .width(Length::Fill)
                .on_press(Event::add_track())
                .style(|theme: &Theme, status| {
                    let palette = theme.extended_palette();
                    let bg = if status == iced_widget::button::Status::Hovered {
                        palette.background.weak.color
                    } else {
                        palette.background.base.color
                    };

                    button::Style {
                        text_color: palette.background.base.text,
                        border: iced_core::Border {
                            radius: 4.0.into(),
                            width: 0.0,
                            color: iced_core::Color::TRANSPARENT,
                        },
                        ..Default::default()
                    }
                    .with_background(bg)
                });

            col = col.push(add_track_container);

            // 使用 scrollable 包裹音轨列表，支持垂直滚动
            let scrollable_content = scrollable(col)
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new().width(8).scroller_width(6),
                ))
                .height(Length::Fill);

            // 当右键菜单或颜色选择器打开时，使用 Stack 覆盖层实现悬浮面板（不挤占 UI）
            let base_content = container(scrollable_content);
            if let Some(target_id) = params.context_menu_target_id {
                if let Some(track_index) = params.tracks.iter().position(|t| t.id == target_id) {
                    // 预估菜单垂直位置：面板顶部内边距(8) + 标题行(12) + 间距(8) + 音轨索引 * 音轨行高(34)
                    let menu_y = 28.0 + track_index as f32 * 34.0;
                    Stack::new()
                        .push(base_content)
                        .push(super::context_menu::background_close_overlay())
                        .push(super::context_menu::positioned_menu(target_id, menu_y))
                        .into()
                } else {
                    base_content.into()
                }
            } else if let Some(target_id) = params.color_picking_track {
                if let Some(track_index) = params.tracks.iter().position(|t| t.id == target_id) {
                    let picker_y = 28.0 + track_index as f32 * 34.0;
                    Stack::new()
                        .push(base_content)
                        .push(super::color_picker::background_close_overlay(target_id))
                        .push(super::color_picker::positioned_panel(target_id, picker_y))
                        .into()
                } else {
                    base_content.into()
                }
            } else {
                base_content.into()
            }
        }
        _ => container(space()).into(),
    };

    // 调整大小手柄
    let is_resizing = params.is_resizing;
    let resize_handle = iced_widget::mouse_area(
        container(space().width(Length::Fixed(RESIZE_HANDLE_WIDTH)))
            .height(Length::Fill)
            .style(move |_theme: &Theme| {
                container::Style::default().background(if is_resizing {
                    palette.primary.strong.color
                } else {
                    palette.background.weakest.color
                })
            }),
    )
    .interaction(iced_core::mouse::Interaction::ResizingHorizontally)
    .on_press(Event::resize_drag_started())
    .on_release(Event::resize_drag_ended());

    // 面板内容 + 调整手柄（手柄在右侧）
    let panel_with_handle = row![
        container(content)
            .width(Length::Fixed(params.panel_width - RESIZE_HANDLE_WIDTH))
            .height(Length::Fill)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style::default().background(palette.background.weakest.color)
            }),
        resize_handle,
    ];

    container(panel_with_handle)
        .width(Length::Fixed(params.panel_width))
        .height(Length::Fill)
        .into()
}

/// 渲染单个音轨行
fn view_track_item<'a>(
    track: &'a Track,
    is_selected: bool,
    window: &'a window::Window,
    renaming_track: Option<&'a (usize, String)>,
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

    let mute_btn = button(
        text("M")
            .size(14)
            .font(iced_core::Font {
                weight: iced_core::font::Weight::Bold,
                ..Default::default()
            })
            .style(move |_theme: &Theme| text::Style {
                color: Some(if track.is_muted {
                    palette.danger.base.color
                } else {
                    text_color
                }),
            }),
    )
    .on_press(Event::track_mute_toggled(track.id))
    .style(|_theme: &Theme, _status| {
        button::Style::default().with_background(iced_core::Color::TRANSPARENT)
    })
    .padding(0);

    let track_row = row![left_icon, space().width(4), name, mute_btn,]
        .align_y(Alignment::Center)
        .padding(4);

    let track_button = button(track_row)
        .width(Length::Fill)
        .on_press(Event::track_selected(track.id))
        .style(move |theme: &Theme, status| {
            let bg = color::track_button_background(track_color, is_selected, status, theme);
            button::Style {
                text_color,
                border: iced_core::Border {
                    radius: 6.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                ..Default::default()
            }
            .with_background(bg)
        });

    // 右键点击按钮打开上下文菜单
    let track_button_with_menu =
        mouse_area(track_button).on_right_press(Event::track_context_menu_opened(track.id));

    column![track_button_with_menu].spacing(2).into()
}
