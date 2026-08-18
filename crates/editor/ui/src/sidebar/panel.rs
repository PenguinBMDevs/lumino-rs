use iced_core::{Alignment, Length, Padding};
use iced_widget::{Stack, button, column, container, mouse_area, row, scrollable, space, text};
use lumino_extras::i18n::{Language, main_translations};

use crate::{
    Element, Theme,
    resources::icon::{self, Icon},
    sidebar::{Event, RESIZE_HANDLE_WIDTH, Route, Track, TrackReorderState},
    window,
};

mod color;
mod track_item;

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
    /// 音轨列表面板空白区域右键菜单是否打开
    pub panel_context_menu_open: bool,
    /// 面板右键菜单位置（窗口逻辑坐标，打开时有效）
    pub panel_context_menu_pos: Option<(f32, f32)>,
    /// 音轨拖拽排序状态（None = 无拖拽进行中）
    pub track_reorder: Option<&'a TrackReorderState>,
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
        Route::File => {
            // 全量渲染所有音轨——由 iced scrollable 原生处理滚动。
            let mut col = column![].spacing(0).padding(8);
            col = col.push(text(t.sidebar_track_list).size(12).style(|theme: &Theme| {
                let palette = theme.extended_palette();
                text::Style {
                    color: Some(palette.background.base.text),
                }
            }));

            let reorder = params.track_reorder;
            let reorder_track_id = reorder.filter(|r| r.active).map(|r| r.track_id);

            // 拖拽激活时渲染插入位置指示分割线
            let insert_divider = || {
                container(space().width(Length::Fill).height(Length::Fixed(3.0))).style(
                    |theme: &Theme| {
                        let palette = theme.extended_palette();
                        container::Style::default().background(palette.primary.strong.color)
                    },
                )
            };

            for (idx, track) in params.tracks.iter().enumerate() {
                if reorder.is_some_and(|r| r.active && r.hover_index == Some(idx)) {
                    col = col.push(insert_divider());
                }
                let is_dragging = reorder_track_id == Some(track.id);
                let track_container = track_item::view_track_item(
                    track,
                    track.id == params.selected_track,
                    window,
                    params.renaming_track,
                    is_dragging,
                );
                col = col.push(track_container);
            }
            if reorder.is_some_and(|r| r.active && r.hover_index == Some(params.tracks.len())) {
                col = col.push(insert_divider());
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

            // base_content 用 mouse_area 包裹：在空白区域右键打开面板级菜单。
            // 音轨行本身的 mouse_area（on_right_press TrackContextMenuOpened）会先
            // `capture_event()` 阻止冒泡，因此只有点击非音轨行的空白区域才会
            // 触发本层的 PanelContextMenuOpened。
            // 拖拽排序：本层 on_move 跟踪鼠标位置更新插入指示；
            // on_release 兜底结束拖拽（行级 mouse_area 不处理释放，不捕获该事件）；
            // on_exit 兜底取消：拖出面板的候选不残留（避免持续驱动每帧重绘）。
            let base_content = mouse_area(container(scrollable_content))
                .on_right_press(Event::panel_context_menu_opened())
                .on_move(|pos| Event::track_reorder_moved(pos.x, pos.y))
                .on_release(Event::track_reorder_ended(None))
                .on_exit(Event::track_reorder_cancelled());

            // 浮动菜单优先级：颜色选择器 > 音轨右键菜单 > 面板空白右键菜单。
            // base_content 作为 Stack 最底层，按需叠加浮动覆盖层。
            let stack = Stack::new().push(base_content);

            if let Some(target_id) = params.color_picking_track {
                if let Some(track_index) = params.tracks.iter().position(|t| t.id == target_id) {
                    let picker_y = 28.0 + track_index as f32 * 34.0;
                    stack
                        .push(super::color_picker::background_close_overlay(target_id))
                        .push(super::color_picker::positioned_panel(target_id, picker_y))
                        .into()
                } else {
                    stack.into()
                }
            } else if let Some(target_id) = params.context_menu_target_id {
                if let Some(track_index) = params.tracks.iter().position(|t| t.id == target_id) {
                    // 预估菜单垂直位置：面板顶部内边距(8) + 标题行(12) + 间距(8) + 音轨索引 * 音轨行高(34)
                    let menu_y = 28.0 + track_index as f32 * 34.0;
                    stack
                        .push(super::context_menu::background_close_overlay())
                        .push(super::context_menu::positioned_menu(target_id, menu_y))
                        .into()
                } else {
                    stack.into()
                }
            } else if params.panel_context_menu_open {
                stack
                    .push(super::panel_context_menu::background_close_overlay())
                    .push(super::panel_context_menu::positioned_menu(
                        params.panel_context_menu_pos,
                    ))
                    .into()
            } else {
                stack.into()
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
