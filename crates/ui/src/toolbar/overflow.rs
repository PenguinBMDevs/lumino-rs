//! 工具栏溢出菜单
//!
//! 当工具栏宽度不足以放下全部功能按钮时，将低优先级项折叠到"更多"(⋮)
//! 按钮的弹出菜单中。菜单以紧凑小框呈现，按钮按自适应网格（多列多行）
//! 排列，框体尺寸贴合内容，不会竖向铺满屏幕；背景色贴近工具栏背景色。

use iced_core::{Alignment, Color, Length, Padding};
use iced_widget::{Space, button, column, container, mouse_area, row, tooltip};

use crate::resources::icon;
use crate::toolbar::{Event, FlipHorizontalMode, Tool, Toolbar};
use crate::{Element, Message, Theme};
use lumino_core::i18n::Language;

/// 图标按钮尺寸（宽高相同）
const BUTTON_SIZE: f32 = 40.0;
/// 图标内部大小
const ICON_SIZE: u32 = 20;
/// 按钮之间的间距
const BUTTON_SPACING: f32 = 4.0;
/// 面板内边距
const PANEL_PADDING: f32 = 8.0;
/// Tooltip 深色背景
const TOOLTIP_BACKGROUND: Color = Color::from_rgba(0.08, 0.08, 0.10, 0.96);
/// 浅色悬停/按下颜色，用于深色按钮背景
const HOVER_BACKGROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.12);
const PRESSED_BACKGROUND: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.22);
/// 浅色文字，用于深色 Tooltip 背景
const TOOLTIP_TEXT_COLOR: Color = Color::from_rgba(0.95, 0.95, 0.95, 1.0);

/// 可折叠工具栏分组标识
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolbarGroup {
    /// 录制按钮
    Record,
    /// 播放控制（快退/播放/暂停/快进）
    Playback,
    /// 循环播放切换
    Loop,
    /// 撤销/重做
    UndoRedo,
    /// 检测仪表盘（性能/时间码显示）
    Dashboard,
    /// 工具选择区（指针/铅笔/橡皮/曲线/量化/变速/翻转/分割/合并/移调/连奏/精度）
    Tools,
    /// 自动滚动模式
    AutoScroll,
    /// 协作按钮
    Collaboration,
}

impl ToolbarGroup {
    /// 分组在工具栏中的显示顺序
    pub const ORDER: &[ToolbarGroup] = &[
        ToolbarGroup::Record,
        ToolbarGroup::Playback,
        ToolbarGroup::Loop,
        ToolbarGroup::UndoRedo,
        ToolbarGroup::Dashboard,
        ToolbarGroup::Tools,
        ToolbarGroup::AutoScroll,
        ToolbarGroup::Collaboration,
    ];

    /// 左侧分组（靠左排列）
    pub const LEFT: &[ToolbarGroup] = &[
        ToolbarGroup::Record,
        ToolbarGroup::Playback,
        ToolbarGroup::Loop,
        ToolbarGroup::UndoRedo,
        ToolbarGroup::Dashboard,
        ToolbarGroup::Tools,
    ];

    /// 右侧分组（靠右排列）
    pub const RIGHT: &[ToolbarGroup] = &[ToolbarGroup::AutoScroll, ToolbarGroup::Collaboration];

    /// 分组收起优先级（数字越小越优先被折叠）
    pub fn collapse_priority(self) -> usize {
        match self {
            ToolbarGroup::Collaboration => 0,
            ToolbarGroup::AutoScroll => 1,
            ToolbarGroup::Dashboard => 2,
            ToolbarGroup::Tools => 3,
            ToolbarGroup::Loop => 4,
            ToolbarGroup::UndoRedo => 5,
            ToolbarGroup::Playback => 6,
            ToolbarGroup::Record => 7,
        }
    }

    /// 分组在工具栏中的预估宽度（px）
    pub fn width(self) -> f32 {
        match self {
            ToolbarGroup::Record => 56.0,
            ToolbarGroup::Playback => 132.0,
            ToolbarGroup::Loop => 40.0,
            ToolbarGroup::UndoRedo => 64.0,
            ToolbarGroup::Dashboard => 288.0,
            ToolbarGroup::Tools => 568.0,
            ToolbarGroup::AutoScroll => 50.0,
            ToolbarGroup::Collaboration => 50.0,
        }
    }

    /// 分组之间的间距（与 `toolbar_view.rs` 中 row! 宏的 spacing 对应）
    pub fn spacing_after(self) -> f32 {
        match self {
            ToolbarGroup::Record => 4.0,
            ToolbarGroup::Playback => 8.0,
            ToolbarGroup::Loop => 8.0,
            ToolbarGroup::UndoRedo => 16.0,
            ToolbarGroup::Dashboard => 16.0,
            ToolbarGroup::Tools => 0.0,
            ToolbarGroup::AutoScroll => 16.0,
            ToolbarGroup::Collaboration => 0.0,
        }
    }
}

/// 溢出菜单中的单个按钮项
pub struct OverflowMenuItem {
    /// 图标
    pub icon: icon::Icon,
    /// 悬浮提示
    pub tooltip: &'static str,
    /// 点击消息
    pub on_press: Message,
    /// 是否禁用（需要选中音符的工具在无选中时置灰）
    pub enabled: bool,
}

impl Toolbar {
    /// 根据可用宽度计算可见分组与隐藏分组
    ///
    /// 返回 `(visible, hidden)`，其中 visible 保持原始顺序，hidden 按优先级排序。
    pub fn compute_overflow_groups(
        &self,
        available_width: f32,
        arrangement_mode: bool,
    ) -> (Vec<ToolbarGroup>, Vec<ToolbarGroup>) {
        // 工程走带模式下工具区宽度更小
        let tools_width = if arrangement_mode {
            200.0
        } else {
            ToolbarGroup::Tools.width()
        };

        // 1. 先尝试全部显示
        let total = ToolbarGroup::ORDER
            .iter()
            .map(|g| {
                let width = if *g == ToolbarGroup::Tools {
                    tools_width
                } else {
                    g.width()
                };
                width + g.spacing_after()
            })
            .sum::<f32>();

        if total <= available_width {
            return (ToolbarGroup::ORDER.to_vec(), Vec::new());
        }

        // 2. 按优先级从低到高排序，得到候选折叠顺序
        let mut candidates = ToolbarGroup::ORDER.to_vec();
        candidates.sort_by_key(|g| g.collapse_priority());

        // 3. 从优先级最低的分组开始折叠，直到剩余分组能放下
        let mut hidden_set = Vec::new();
        let mut remaining_width = total;
        for group in candidates {
            if remaining_width <= available_width {
                break;
            }
            let width = if group == ToolbarGroup::Tools {
                tools_width
            } else {
                group.width()
            };
            let removed = width + group.spacing_after();
            remaining_width -= removed;
            hidden_set.push(group);
        }

        let visible: Vec<ToolbarGroup> = ToolbarGroup::ORDER
            .iter()
            .copied()
            .filter(|g| !hidden_set.contains(g))
            .collect();

        (visible, hidden_set)
    }

    /// 将某个隐藏分组展开为溢出菜单项列表
    pub fn group_overflow_items(
        &self,
        group: ToolbarGroup,
        has_selection: bool,
        language: Language,
        arrangement_mode: bool,
    ) -> Vec<OverflowMenuItem> {
        let t = lumino_core::i18n::main_translations(language);
        let ctrl = self.ctrl_pressed;
        let shift = self.shift_pressed;

        match group {
            ToolbarGroup::Record => vec![OverflowMenuItem {
                icon: icon::PlayCircle,
                tooltip: if self.is_recording {
                    t.record_stop
                } else {
                    t.record_start
                },
                on_press: if self.is_recording {
                    Event::record_stop()
                } else {
                    Event::record()
                },
                enabled: true,
            }],
            ToolbarGroup::Playback => vec![
                OverflowMenuItem {
                    icon: icon::SkipBackward,
                    tooltip: t.skip_backward,
                    on_press: Event::skip_backward(),
                    enabled: true,
                },
                OverflowMenuItem {
                    icon: if self.is_playing {
                        icon::Pause
                    } else {
                        icon::Play
                    },
                    tooltip: if self.is_playing { t.pause } else { t.play },
                    on_press: if self.is_playing {
                        Event::pause()
                    } else {
                        Event::play()
                    },
                    enabled: true,
                },
                OverflowMenuItem {
                    icon: icon::SkipForward,
                    tooltip: t.skip_forward,
                    on_press: Event::skip_forward(),
                    enabled: true,
                },
            ],
            ToolbarGroup::Loop => vec![OverflowMenuItem {
                icon: if self.is_looping {
                    icon::ArrowsLeftRight
                } else {
                    icon::Ban
                },
                tooltip: if self.is_looping {
                    t.loop_on
                } else {
                    t.loop_off
                },
                on_press: Event::toggle_loop(),
                enabled: true,
            }],
            ToolbarGroup::UndoRedo => vec![
                OverflowMenuItem {
                    icon: icon::Undo,
                    tooltip: t.undo,
                    on_press: Event::undo(),
                    enabled: true,
                },
                OverflowMenuItem {
                    icon: icon::Redo,
                    tooltip: t.redo,
                    on_press: Event::redo(),
                    enabled: true,
                },
            ],
            ToolbarGroup::Dashboard => {
                // 仪表盘是只读信息，折叠后不在菜单中重复展示
                Vec::new()
            }
            ToolbarGroup::Tools => {
                if arrangement_mode {
                    return vec![
                        OverflowMenuItem {
                            icon: icon::MousePointer,
                            tooltip: t.tool_pointer,
                            on_press: Event::tool_selected(Tool::Pointer),
                            enabled: true,
                        },
                        OverflowMenuItem {
                            icon: icon::Curve,
                            tooltip: t.tool_curve,
                            on_press: Event::tool_selected(Tool::Curve),
                            enabled: true,
                        },
                        OverflowMenuItem {
                            icon: icon::Eraser,
                            tooltip: t.tool_eraser,
                            on_press: Event::tool_selected(Tool::Eraser),
                            enabled: true,
                        },
                    ];
                }

                let (transpose_down_tooltip, transpose_down_event) = if ctrl {
                    (t.tool_transpose_down_octave, Event::transpose_down(12))
                } else {
                    (t.tool_transpose_down, Event::transpose_down(1))
                };
                let (transpose_up_tooltip, transpose_up_event) = if ctrl {
                    (t.tool_transpose_up_octave, Event::transpose_up(12))
                } else {
                    (t.tool_transpose_up, Event::transpose_up(1))
                };
                let flip_horizontal_event = if shift {
                    Event::flip_horizontal(FlipHorizontalMode::Right)
                } else if ctrl {
                    Event::flip_horizontal(FlipHorizontalMode::Left)
                } else {
                    Event::flip_horizontal(FlipHorizontalMode::Center)
                };

                vec![
                    OverflowMenuItem {
                        icon: icon::MousePointer,
                        tooltip: t.tool_pointer,
                        on_press: Event::tool_selected(Tool::Pointer),
                        enabled: true,
                    },
                    OverflowMenuItem {
                        icon: icon::Pencil,
                        tooltip: t.tool_pencil,
                        on_press: Event::tool_selected(Tool::Pencil),
                        enabled: true,
                    },
                    OverflowMenuItem {
                        icon: icon::Eraser,
                        tooltip: t.tool_eraser,
                        on_press: Event::tool_selected(Tool::Eraser),
                        enabled: true,
                    },
                    OverflowMenuItem {
                        icon: icon::Curve,
                        tooltip: t.tool_curve,
                        on_press: Event::tool_selected(Tool::Curve),
                        enabled: true,
                    },
                    OverflowMenuItem {
                        icon: icon::Quantize,
                        tooltip: t.tool_quantize,
                        on_press: Event::quantize(),
                        enabled: true,
                    },
                    OverflowMenuItem {
                        icon: icon::Speed,
                        tooltip: t.tool_speed,
                        on_press: Event::speed_change(),
                        enabled: true,
                    },
                    OverflowMenuItem {
                        icon: icon::FlipVertical,
                        tooltip: t.tool_flip_vertical,
                        on_press: Event::flip_vertical(),
                        enabled: has_selection,
                    },
                    OverflowMenuItem {
                        icon: icon::FlipHorizontal,
                        tooltip: t.tool_flip_horizontal,
                        on_press: flip_horizontal_event,
                        enabled: has_selection,
                    },
                    OverflowMenuItem {
                        icon: icon::Split,
                        tooltip: t.tool_split,
                        on_press: Event::split(),
                        enabled: true,
                    },
                    OverflowMenuItem {
                        icon: icon::Glue,
                        tooltip: t.tool_glue,
                        on_press: Event::glue(),
                        enabled: true,
                    },
                    OverflowMenuItem {
                        icon: icon::TransposeDown,
                        tooltip: transpose_down_tooltip,
                        on_press: transpose_down_event,
                        enabled: has_selection,
                    },
                    OverflowMenuItem {
                        icon: icon::TransposeUp,
                        tooltip: transpose_up_tooltip,
                        on_press: transpose_up_event,
                        enabled: has_selection,
                    },
                    OverflowMenuItem {
                        icon: icon::Tie,
                        tooltip: t.tool_tie,
                        on_press: Event::tie(),
                        enabled: true,
                    },
                ]
            }
            ToolbarGroup::AutoScroll => vec![OverflowMenuItem {
                icon: match self.auto_scroll_mode {
                    lumino_core::storage::config::AutoScrollMode::FixedIndicatorLeft => {
                        icon::ArrowsLeftRight
                    }
                    lumino_core::storage::config::AutoScrollMode::ScrollingIndicator => {
                        icon::Scroll
                    }
                    lumino_core::storage::config::AutoScrollMode::Off => icon::Ban,
                },
                tooltip: t.auto_scroll_tooltip,
                on_press: Event::auto_scroll_mode_changed(),
                enabled: true,
            }],
            ToolbarGroup::Collaboration => vec![OverflowMenuItem {
                icon: icon::Users,
                tooltip: t.collaboration_tooltip,
                on_press: Event::open_collaboration_dialog(),
                enabled: true,
            }],
        }
    }

    /// 渲染溢出菜单面板
    ///
    /// 所有隐藏分组被展开为图标按钮，按可见分组顺序排列。
    ///
    /// # 参数
    /// - `panel_background`: 面板背景色，贴近工具栏背景色
    /// - `theme`: 当前主题，用于图标颜色渲染
    ///
    /// # 尺寸计算
    /// 面板为紧凑小框，按钮按自适应网格（多列多行）排列，框体尺寸贴合内容，
    /// 不会竖向铺满。列数取接近 √n 的值，使整体接近正方形且不过宽。
    pub fn render_overflow_menu<'a>(
        &'a self,
        hidden_groups: &[ToolbarGroup],
        has_selection: bool,
        language: Language,
        panel_background: iced_core::Color,
        theme: &'a Theme,
        arrangement_mode: bool,
    ) -> Element<'a> {
        let items: Vec<OverflowMenuItem> = hidden_groups
            .iter()
            .flat_map(|g| self.group_overflow_items(*g, has_selection, language, arrangement_mode))
            .collect();

        let buttons = items
            .into_iter()
            .map(|item| overflow_menu_button(item, theme))
            .collect::<Vec<_>>();

        let button_count = buttons.len().max(1);
        // 列数：取接近 √n 的整数，保证框体接近正方形且紧凑
        let cols = (button_count as f32).sqrt().ceil() as usize;
        let cols = cols.max(1);
        let rows = button_count.div_ceil(cols);

        // 按列构建：每列是一个纵向 button 列，列间横向排列成网格
        let mut btn_iter = buttons.into_iter();
        let mut columns: Vec<Element<'a>> = Vec::with_capacity(cols);
        for _ in 0..cols {
            let col_buttons: Vec<Element<'a>> = btn_iter.by_ref().take(rows).collect();
            if col_buttons.is_empty() {
                break;
            }
            let col = column(col_buttons)
                .spacing(BUTTON_SPACING)
                .align_x(Alignment::Center);
            columns.push(col.into());
        }

        let grid = row(columns)
            .spacing(BUTTON_SPACING)
            .align_y(Alignment::Start);

        let panel_width = cols as f32 * BUTTON_SIZE
            + (cols.saturating_sub(1)) as f32 * BUTTON_SPACING
            + PANEL_PADDING * 2.0;
        let panel_height = rows as f32 * BUTTON_SIZE
            + (rows.saturating_sub(1)) as f32 * BUTTON_SPACING
            + PANEL_PADDING * 2.0;

        let panel = container(grid)
            .padding(PANEL_PADDING)
            .width(Length::Fixed(panel_width))
            .height(Length::Fixed(panel_height))
            .style(move |_theme: &Theme| container::Style {
                background: Some(iced_core::Background::Color(panel_background)),
                border: iced_core::Border::default().rounded(8),
                ..Default::default()
            });

        // 吞掉菜单面板内部的点击事件，避免触发下层的关闭覆盖层
        mouse_area(panel).on_press(Message::Null).into()
    }
}

/// 构建单个溢出菜单按钮
fn overflow_menu_button(item: OverflowMenuItem, theme: &Theme) -> Element<'static> {
    // 图标按当前主题渲染，保证在工具栏背景色面板上可见
    let icon = icon::view_with_size_and_theme(item.icon, ICON_SIZE, ICON_SIZE, Some(theme));

    let btn = button(icon)
        .width(Length::Fixed(BUTTON_SIZE))
        .height(Length::Fixed(BUTTON_SIZE))
        .style(move |_theme: &Theme, status| button_style(status, item.enabled));

    let btn = if item.enabled {
        btn.on_press(item.on_press)
    } else {
        btn
    };

    tooltip::Tooltip::new(btn, item.tooltip, tooltip::Position::Right)
        .style(tooltip_style)
        .into()
}

/// 按钮样式（无选中/禁用时背景透明）
fn button_style(status: button::Status, enabled: bool) -> button::Style {
    use button::Status;

    let background = if !enabled {
        Color::TRANSPARENT
    } else {
        match status {
            Status::Hovered => HOVER_BACKGROUND,
            Status::Pressed => PRESSED_BACKGROUND,
            _ => Color::TRANSPARENT,
        }
    };

    button::Style {
        border: iced_core::Border::default().rounded(6),
        ..Default::default()
    }
    .with_background(background)
}

/// Tooltip 样式：深色背景 + 浅色文字
fn tooltip_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(iced_core::Background::Color(TOOLTIP_BACKGROUND)),
        border: iced_core::Border::default().rounded(4),
        text_color: Some(TOOLTIP_TEXT_COLOR),
        ..Default::default()
    }
}

/// 关闭背景：点击菜单外部区域关闭
///
/// 作为 Stack 的底层，覆盖整个父区域，点击时关闭菜单。
pub fn background_close_overlay<'a>() -> Element<'a> {
    mouse_area(Space::new().width(Length::Fill).height(Length::Fill))
        .on_press(Event::close_overflow_menu())
        .into()
}

/// 将菜单面板定位在容器右上角
pub fn positioned_overflow_menu<'a>(menu: Element<'a>, toolbar_height: f32) -> Element<'a> {
    container(menu)
        .padding(Padding {
            top: toolbar_height,
            right: 4.0,
            bottom: 0.0,
            left: 0.0,
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced_core::alignment::Horizontal::Right)
        .align_y(iced_core::alignment::Vertical::Top)
        .into()
}
