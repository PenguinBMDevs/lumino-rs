use iced_core::{Alignment, Color, Length, Padding};
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
            // 音轨总览模式下左侧面板已隐藏，此处返回空
            container(space()).into()
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
                    params.color_picking_track,
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

            // 当右键菜单打开时，使用 Stack 覆盖层实现悬浮菜单（不挤占 UI）
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

/// 预设音轨颜色
const TRACK_COLORS: [Color; 8] = [
    Color::from_rgb(0.85, 0.15, 0.15),
    Color::from_rgb(0.15, 0.75, 0.35),
    Color::from_rgb(0.15, 0.45, 0.85),
    Color::from_rgb(0.85, 0.75, 0.10),
    Color::from_rgb(0.75, 0.15, 0.75),
    Color::from_rgb(0.15, 0.75, 0.75),
    Color::from_rgb(0.95, 0.50, 0.15),
    Color::from_rgb(0.50, 0.50, 0.50),
];

/// 渲染单个音轨行
fn view_track_item<'a>(
    track: &'a Track,
    is_selected: bool,
    window: &'a window::Window,
    renaming_track: Option<&'a (usize, String)>,
    color_picking_track: Option<usize>,
) -> Element<'a> {
    let palette = window.theme.extended_palette();
    let is_renaming = renaming_track.map(|(id, _)| *id) == Some(track.id);
    let is_color_picking = color_picking_track == Some(track.id);

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
        // 如果设置了选项卡颜色，显示颜色块；否则显示通道标签（如 A01/B02）
        let icon_content: Element<'a> = if let Some(color) = track.color {
            container(space().width(16).height(16))
                .style(move |_theme: &Theme| container::Style {
                    background: Some(iced_core::Background::Color(color)),
                    border: iced_core::Border::default().rounded(4),
                    ..Default::default()
                })
                .into()
        } else {
            text(&track.display_label)
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
                })
                .into()
        };

        container(icon_content)
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
        text(&track.name).size(14).width(Length::Fill).into()
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
                    palette.background.strong.color
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
        });

    // 右键点击按钮打开上下文菜单
    let track_button_with_menu =
        mouse_area(track_button).on_right_press(Event::track_context_menu_opened(track.id));

    let mut col = column![track_button_with_menu].spacing(2);

    if is_renaming {
        // 文字输入框已有 on_submit 处理 Enter 键确认，无需额外按钮
    }

    if is_color_picking {
        let color_buttons = TRACK_COLORS
            .into_iter()
            .map(|color| {
                button(space().width(20).height(20))
                    .on_press(Event::track_color_selected(track.id, color))
                    .style(move |_theme: &Theme, _status| button::Style {
                        background: Some(iced_core::Background::Color(color)),
                        border: iced_core::Border::default().rounded(4),
                        ..Default::default()
                    })
                    .into()
            })
            .collect::<Vec<_>>();
        let color_row = row(color_buttons).spacing(4).wrap();
        let close_btn = button(text("取消").size(12))
            .on_press(Event::track_color_picker_closed(track.id))
            .style(|_theme: &Theme, _status| {
                button::Style::default().with_background(palette.background.strong.color)
            });
        col = col.push(color_row);
        col = col.push(close_btn);
    }

    col.into()
}
