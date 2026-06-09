//! Toolbar 视图渲染子模块

use iced_core::Alignment;
use iced_widget::{button, column, container, mouse_area, pick_list, row, space, text};

use crate::resources::icon;
use crate::toolbar::buttons::{flip_button, tool_button, tool_selector};
use crate::toolbar::{Event, FlipHorizontalMode, NotePrecision, RESIZE_HANDLE_HEIGHT, Tool};
use crate::{Element, Message, Theme, window};

use super::Toolbar;

impl Toolbar {
    /// 渲染工具栏视图
    pub fn view<'a>(&'a self, window: &'a window::Window, has_selection: bool) -> Element<'a> {
        let palette = window.theme.extended_palette();

        // 计算内容区域高度（总高度减去手柄高度）
        let content_height = self.height - RESIZE_HANDLE_HEIGHT;

        // 录制按钮区域
        let record_button = self.render_record_button(content_height, palette, window);

        // 播放控制区域 (132px 宽)
        let playback_controls = container(
            row![
                tool_button(icon::SkipBackward, "快退", Event::skip_backward(), window),
                space().width(4),
                if self.is_playing {
                    tool_button(icon::Pause, "暂停", Event::pause(), window)
                } else {
                    tool_button(icon::Play, "播放", Event::play(), window)
                },
                space().width(4),
                tool_button(icon::SkipForward, "快进", Event::skip_forward(), window),
            ]
            .align_y(Alignment::Center),
        )
        .width(132)
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .align_x(iced_core::alignment::Horizontal::Center)
        .style(move |_theme: &Theme| {
            container::Style::default()
                .background(palette.background.weak.color)
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                })
        });

        // 循环按钮区域
        use crate::widget;
        let loop_button = container(widget::with_tooltip_bottom(
            button(
                row![icon::view_with_size_and_theme(
                    if self.is_looping {
                        icon::ArrowsLeftRight
                    } else {
                        icon::Ban
                    },
                    20,
                    20,
                    Some(&window.theme),
                )]
                .align_y(Alignment::Center),
            )
            .on_press(Event::toggle_loop())
            .style(move |_theme: &Theme, status| {
                let bg = if self.is_looping {
                    palette.primary.base.color
                } else if status == iced_widget::button::Status::Hovered {
                    palette.background.weak.color
                } else {
                    iced_core::Color::TRANSPARENT
                };
                button::Style {
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        width: 0.0,
                        color: iced_core::Color::TRANSPARENT,
                    },
                    ..Default::default()
                }
                .with_background(bg)
            })
            .padding(4),
            if self.is_looping {
                "循环播放: 开"
            } else {
                "循环播放: 关"
            },
        ))
        .width(40)
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .align_x(iced_core::alignment::Horizontal::Center)
        .style(move |_theme: &Theme| {
            container::Style::default()
                .background(if self.is_looping {
                    palette.primary.weak.color
                } else {
                    palette.background.weak.color
                })
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                })
        });

        // 工具选择区域
        let tools = container(
            row![
                tool_selector(
                    icon::MousePointer,
                    "选择工具",
                    Tool::Pointer,
                    self.current_tool,
                    window
                ),
                space().width(4),
                tool_selector(
                    icon::Pencil,
                    "铅笔工具",
                    Tool::Pencil,
                    self.current_tool,
                    window
                ),
                space().width(4),
                tool_selector(
                    icon::Eraser,
                    "橡皮擦",
                    Tool::Eraser,
                    self.current_tool,
                    window
                ),
                space().width(4),
                tool_button(icon::Quantize, "量化", Event::quantize(), window),
                space().width(4),
                // 变速按钮始终可点击：Ctrl+Click 打开变速对话框不需要选中音符。
                // 普通点击的无选中情况由 handler 内部的 selected.is_empty() 兜底。
                flip_button(
                    icon::Speed,
                    "变速",
                    Event::speed_change(),
                    true,
                    window
                ),
                space().width(4),
                flip_button(
                    icon::FlipVertical,
                    "垂直翻转",
                    Event::flip_vertical(),
                    has_selection,
                    window
                ),
                space().width(4),
                flip_button(
                    icon::FlipHorizontal,
                    "水平翻转",
                    if self.shift_pressed {
                        Event::flip_horizontal(FlipHorizontalMode::Right)
                    } else if self.ctrl_pressed {
                        Event::flip_horizontal(FlipHorizontalMode::Left)
                    } else {
                        Event::flip_horizontal(FlipHorizontalMode::Center)
                    },
                    has_selection,
                    window
                ),
                space().width(8),
                // 分割/合并按钮
                tool_button(icon::Split, "分割(Split)", Event::split(), window),
                space().width(4),
                tool_button(icon::Glue, "合并(Glue)", Event::glue(), window),
                space().width(8),
                // 移调按钮
                flip_button(
                    icon::TransposeDown,
                    "移调 -1",
                    Event::transpose_down(),
                    has_selection,
                    window
                ),
                space().width(4),
                flip_button(
                    icon::TransposeUp,
                    "移调 +1",
                    Event::transpose_up(),
                    has_selection,
                    window
                ),
            ]
            .align_y(Alignment::Center),
        )
        .width(560)
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .align_x(iced_core::alignment::Horizontal::Center)
        .style(move |_theme: &Theme| {
            container::Style::default()
                .background(palette.background.weak.color)
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                })
        });

        // 调整大小手柄区域
        let resize_handle: Element<'_> = mouse_area(
            container(space().height(iced_widget::core::Length::Fixed(RESIZE_HANDLE_HEIGHT)))
                .width(iced_widget::core::Length::Fill)
                .style(move |_theme: &Theme| {
                    container::Style::default().background(if self.is_resizing {
                        palette.primary.strong.color
                    } else {
                        palette.background.weakest.color
                    })
                }),
        )
        .interaction(iced_core::mouse::Interaction::ResizingVertically)
        .on_press(Message::Toolbar(Event::ResizeDragStarted(
            iced_core::Point::new(0.0, 0.0),
        )))
        .on_release(Message::Toolbar(Event::ResizeDragEnded))
        .into();

        // 精度设置区域
        let precision_options: Vec<NotePrecision> = NotePrecision::presets()
            .iter()
            .copied()
            .chain(std::iter::once(NotePrecision::Custom))
            .collect();

        let precision_selector = container(
            row![
                text("精度:").size(14),
                space().width(8),
                pick_list(precision_options, Some(self.note_precision), |precision| {
                    if precision == NotePrecision::Custom {
                        // 选择自定义时，发送消息到Root打开对话框
                        Message::OpenCustomPrecisionDialog
                    } else {
                        Event::precision_changed(precision)
                    }
                },)
                .placeholder("选择精度")
                .padding([4, 8])
                .width(iced_widget::core::Length::Fixed(120.0)),
            ]
            .align_y(Alignment::Center),
        )
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .padding([0, 16])
        .style(move |_theme: &Theme| {
            container::Style::default().background(palette.background.weakest.color)
        });

        // 自动滚动按钮区域
        use lumino_core::storage::config::AutoScrollMode;
        let auto_scroll_label = match self.auto_scroll_mode {
            AutoScrollMode::FixedIndicatorLeft => "自动滚动: 固定",
            AutoScrollMode::ScrollingIndicator => "自动滚动: 滚动",
            AutoScrollMode::Off => "自动滚动: 关闭",
        };
        let auto_scroll_icon = match self.auto_scroll_mode {
            AutoScrollMode::FixedIndicatorLeft => icon::ArrowsLeftRight,
            AutoScrollMode::ScrollingIndicator => icon::Scroll,
            AutoScrollMode::Off => icon::Ban,
        };
        let auto_scroll_button = container(widget::with_tooltip_bottom(
            button(
                row![
                    icon::view_with_size_and_theme(auto_scroll_icon, 18, 18, Some(&window.theme)),
                    space().width(6),
                    text(auto_scroll_label)
                        .size(14)
                        .color(palette.background.weakest.text),
                ]
                .align_y(Alignment::Center),
            )
            .on_press(Event::auto_scroll_mode_changed())
            .style(move |_theme: &Theme, status| {
                let bg = match status {
                    iced_widget::button::Status::Hovered => palette.background.weak.color,
                    _ => palette.background.weakest.color,
                };
                button::Style {
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        width: 0.0,
                        color: iced_core::Color::TRANSPARENT,
                    },
                    ..Default::default()
                }
                .with_background(bg)
            })
            .padding([8, 12]),
            "切换自动滚动模式",
        ))
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .padding([0, 16])
        .style(move |_theme: &Theme| {
            container::Style::default().background(palette.background.weakest.color)
        });

        // 协作按钮区域
        let collaboration_button = container(widget::with_tooltip_bottom(
            button(
                row![
                    icon::view_with_size_and_theme(icon::Users, 18, 18, Some(&window.theme)),
                    space().width(6),
                    text("多人协作")
                        .size(14)
                        .color(palette.background.weakest.text),
                ]
                .align_y(Alignment::Center),
            )
            .on_press(Event::open_collaboration_dialog())
            .style(move |_theme: &Theme, status| {
                let bg = match status {
                    iced_widget::button::Status::Hovered => palette.background.weak.color,
                    _ => palette.background.weakest.color,
                };
                button::Style {
                    border: iced_core::Border {
                        radius: 4.0.into(),
                        width: 0.0,
                        color: iced_core::Color::TRANSPARENT,
                    },
                    ..Default::default()
                }
                .with_background(bg)
            })
            .padding([8, 12]),
            "打开协作面板",
        ))
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .padding([0, 16])
        .style(move |_theme: &Theme| {
            container::Style::default().background(palette.background.weakest.color)
        });

        // 撤销/重做按钮区域
        let undo_redo_controls = container(
            row![
                tool_button(icon::Undo, "撤销", Event::undo(), window),
                space().width(4),
                tool_button(icon::Redo, "重做", Event::redo(), window),
            ]
            .align_y(Alignment::Center),
        )
        .width(64)
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .align_x(iced_core::alignment::Horizontal::Center)
        .style(move |_theme: &Theme| {
            container::Style::default()
                .background(palette.background.weak.color)
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                })
        });

        // 主工具栏内容 - 横向排列所有区域，协作按钮在最右边
        let toolbar_content = container(
            row![
                record_button,
                space().width(4),
                playback_controls,
                space().width(8),
                loop_button,
                space().width(8),
                undo_redo_controls,
                space().width(16),
                tools,
                space().width(16),
                precision_selector,
                space().width(iced_widget::core::Length::Fill),
                auto_scroll_button,
                space().width(16),
                collaboration_button,
            ]
            .align_y(Alignment::Center),
        )
        .width(iced_widget::core::Length::Fill)
        .height(iced_widget::core::Length::Fixed(content_height))
        .padding([8, 16])
        .style(move |_theme: &Theme| {
            container::Style::default().background(palette.background.weakest.color)
        });

        // 组合工具栏内容和调整手柄
        column![toolbar_content, resize_handle]
            .width(iced_widget::core::Length::Fill)
            .height(iced_widget::core::Length::Fixed(self.height))
            .into()
    }
}
