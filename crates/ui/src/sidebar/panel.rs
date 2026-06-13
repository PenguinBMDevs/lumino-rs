use iced_core::{Alignment, Length, Padding};
use iced_widget::{button, column, container, row, scrollable, space, text};

use crate::{
    Element, Theme,
    resources::icon::{self, Icon},
    sidebar::{Event, RESIZE_HANDLE_WIDTH, Route, Track},
    window,
};

/// 侧边栏视图参数
#[derive(Clone)]
pub struct SidebarViewParams<'a> {
    pub route: Route,
    pub tracks: &'a [Track],
    pub selected_track: usize,
    pub add_track_menu_open: bool,
    pub panel_width: f32,
    pub is_resizing: bool,
    pub scroll_offset: f32,
}

pub fn view<'a>(params: SidebarViewParams<'a>, window: &'a window::Window) -> Element<'a> {
    let palette = window.theme.extended_palette();

    let content: Element<'a> = match params.route {
        Route::Arrangement => {
            // 音轨总览模式下左侧面板已隐藏，此处返回空
            container(space()).into()
        }
        Route::File => {
            // === 虚拟滚动 ===
            // 825 轨全部生成 iced widget 会导致每帧 8000+ widget 树重建。
            // 只渲染视口附近 ~30 条，其余用 spacer 占高度。
            const TRACK_HEIGHT: f32 = 32.0;
            const VISIBLE_COUNT: usize = 30; // 一次渲染 ~30 条
            const BUFFER: usize = 5; // 上下各多 5 条防白边

            let first_visible =
                ((params.scroll_offset / TRACK_HEIGHT) as usize).saturating_sub(BUFFER);
            let first_visible = first_visible.min(params.tracks.len());
            let last_visible =
                (first_visible + VISIBLE_COUNT + BUFFER * 2).min(params.tracks.len());

            let mut col = column![].spacing(0).padding(8);

            // 音轨列表标题
            col = col.push(text("音轨列表").size(12).style(|theme: &Theme| {
                let palette = theme.extended_palette();
                text::Style {
                    color: Some(palette.background.base.text),
                }
            }));

            // 顶部占位（已滚出视口的音轨）
            if first_visible > 0 {
                col = col.push(space().width(iced_core::Length::Fixed(0.0)).height(
                    iced_core::Length::Fixed(first_visible as f32 * TRACK_HEIGHT),
                ));
            }

            for track in &params.tracks[first_visible..last_visible] {
                let track_container =
                    view_track_item(track, track.id == params.selected_track, window);
                col = col.push(track_container);
            }

            // 添加音轨按钮区域
            if params.add_track_menu_open {
                // 展开状态：显示更多选项按钮
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
                    text("添加音轨").size(14).width(Length::Fill),
                    container(
                        button(icon::view_with_size_and_theme(
                            Icon::EllipsisVertical,
                            15,
                            15,
                            Some(&window.theme),
                        ))
                        .on_press(Event::add_track())
                        .style(|_theme: &Theme, _status| {
                            button::Style::default()
                                .with_background(iced_core::Color::from_rgb(0.84, 0.84, 0.84))
                        })
                        .padding(2)
                    )
                    .style(|theme: &Theme| {
                        let palette = theme.extended_palette();
                        container::Style::default()
                            .background(palette.background.weak.color)
                            .border(iced_core::Border {
                                radius: 4.0.into(),
                                width: 0.0,
                                color: iced_core::Color::TRANSPARENT,
                            })
                    })
                    .padding(2),
                ]
                .align_y(Alignment::Center)
                .padding(6);

                let add_track_container = button(add_track_row)
                    .width(Length::Fill)
                    .on_press(Event::add_track_menu_toggled())
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
            } else {
                // 收起状态：简单的添加音轨行
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
                    text("添加音轨").size(14).width(Length::Fill),
                    container(
                        button(icon::view_with_size_and_theme(
                            Icon::EllipsisVertical,
                            18,
                            18,
                            Some(&window.theme),
                        ))
                        .on_press(Event::add_track_menu_toggled())
                        .style(|_theme: &Theme, _status| {
                            button::Style::default().with_background(iced_core::Color::TRANSPARENT)
                        })
                        .padding(0)
                    ),
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
            }

            // 底部占位（未渲染的音轨高度）
            let remaining = params.tracks.len() - last_visible;
            if remaining > 0 {
                col = col.push(
                    space()
                        .width(iced_core::Length::Fixed(0.0))
                        .height(iced_core::Length::Fixed(remaining as f32 * TRACK_HEIGHT)),
                );
            }

            // 使用 scrollable 包裹音轨列表，支持垂直滚动 + 虚拟滚动
            let scrollable_content = scrollable(col)
                .on_scroll(|viewport| {
                    crate::Message::Sidebar(Event::TrackScrolled(viewport.absolute_offset().y))
                })
                .direction(scrollable::Direction::Vertical(
                    scrollable::Scrollbar::new().width(8).scroller_width(6),
                ))
                .height(Length::Fill);

            container(scrollable_content).into()
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
) -> Element<'a> {
    let palette = window.theme.extended_palette();

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
            text("A01")
                .size(14)
                .font(iced_core::Font {
                    weight: iced_core::font::Weight::Bold,
                    ..Default::default()
                })
                .style(move |theme: &Theme| {
                    let p = theme.extended_palette();
                    let c = if is_selected {
                        p.background.base.color
                    } else {
                        p.background.strong.color
                    };
                    text::Style { color: Some(c) }
                }),
        )
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
    };

    let name = text(&track.name).size(14).width(Length::Fill);

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
                    palette.background.strong.color
                }),
            }),
    )
    .on_press(Event::track_mute_toggled(track.id))
    .style(|_theme: &Theme, _status| {
        button::Style::default().with_background(iced_core::Color::TRANSPARENT)
    })
    .padding(0);

    let eye_icon = if track.is_onion_skin_on {
        Icon::Eye
    } else {
        Icon::EyeSlash
    };

    let eye_btn = button(icon::view_with_size_and_theme(
        eye_icon,
        16,
        16,
        Some(&window.theme),
    ))
    .on_press(Event::track_onion_skin_toggled(track.id))
    .style(|_theme: &Theme, _status| {
        button::Style::default().with_background(iced_core::Color::TRANSPARENT)
    })
    .padding(0);

    let track_row = row![
        left_icon,
        space().width(4),
        name,
        mute_btn,
        space().width(4),
        eye_btn,
    ]
    .align_y(Alignment::Start)
    .padding(4);

    button(track_row)
        .width(Length::Fill)
        .on_press(Event::track_selected(track.id))
        .style(move |theme: &Theme, status| {
            let p = theme.extended_palette();
            let bg = if is_selected {
                p.background.strong.color
            } else if status == iced_widget::button::Status::Hovered {
                p.background.weak.color
            } else {
                p.background.base.color
            };
            button::Style {
                text_color: p.background.base.text,
                border: iced_core::Border {
                    radius: 6.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                },
                ..Default::default()
            }
            .with_background(bg)
        })
        .into()
}
