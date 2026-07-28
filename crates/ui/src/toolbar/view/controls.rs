//! 工具栏控件渲染函数
//!
//! 包含播放控制、循环按钮、工具选择区域、调整手柄、撤销/重做等控件渲染。

use iced_core::Alignment;
use iced_widget::{button, container, mouse_area, row, space};

use crate::resources::icon;
use crate::toolbar::buttons::{flip_button, tool_button, tool_selector};
use crate::toolbar::{ButtonId, Event, FlipHorizontalMode, RESIZE_HANDLE_HEIGHT, Tool, Toolbar};
use crate::widget;
use crate::{Element, Message, Theme, window};
use lumino_core::i18n::{Language, MainTranslations};

impl Toolbar {
    /// 渲染播放控制区域（SkipBack / PlayPause / SkipForward），132px 宽
    pub fn render_playback_controls<'a>(
        &'a self,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        t: &'static MainTranslations,
        window: &'a window::Window,
    ) -> Element<'a> {
        container(
            row![
                tool_button(
                    icon::SkipBackward,
                    t.skip_backward,
                    Event::skip_backward(),
                    window,
                    Some(Event::button_hovered(Some(ButtonId::SkipBackward))),
                ),
                space().width(4),
                if self.is_playing {
                    tool_button(
                        icon::Pause,
                        t.pause,
                        Event::pause(),
                        window,
                        Some(Event::button_hovered(Some(ButtonId::Pause))),
                    )
                } else {
                    tool_button(
                        icon::Play,
                        t.play,
                        Event::play(),
                        window,
                        Some(Event::button_hovered(Some(ButtonId::Play))),
                    )
                },
                space().width(4),
                tool_button(
                    icon::SkipForward,
                    t.skip_forward,
                    Event::skip_forward(),
                    window,
                    Some(Event::button_hovered(Some(ButtonId::SkipForward))),
                ),
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
        })
        .into()
    }

    /// 渲染循环播放切换按钮，40px 宽
    pub fn render_loop_button<'a>(
        &'a self,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        t: &'static MainTranslations,
        window: &'a window::Window,
    ) -> Element<'a> {
        container(widget::with_tooltip_bottom(
            iced_widget::mouse_area(
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
            )
            .on_enter(Event::button_hovered(Some(ButtonId::Loop)))
            .on_exit(Event::button_hovered(None)),
            if self.is_looping {
                t.loop_on
            } else {
                t.loop_off
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
        })
        .into()
    }

    /// 渲染工具选择区域（指针/铅笔/橡皮/量化/变速/翻转/分割/合并/移调 + 精度下拉），宽度自适应
    #[allow(clippy::too_many_arguments)]
    pub fn render_tools_section<'a>(
        &'a self,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        has_selection: bool,
        t: &'static MainTranslations,
        window: &'a window::Window,
        language: Language,
        arrangement_mode: bool,
    ) -> Element<'a> {
        if arrangement_mode {
            return self.render_arrangement_tools_section(
                content_height,
                palette,
                has_selection,
                t,
                window,
            );
        }

        let (transpose_down_tooltip, transpose_down_event) = if self.ctrl_pressed {
            (t.tool_transpose_down_octave, Event::transpose_down(12))
        } else {
            (t.tool_transpose_down, Event::transpose_down(1))
        };
        let (transpose_up_tooltip, transpose_up_event) = if self.ctrl_pressed {
            (t.tool_transpose_up_octave, Event::transpose_up(12))
        } else {
            (t.tool_transpose_up, Event::transpose_up(1))
        };

        container(
            row![
                tool_selector(
                    icon::MousePointer,
                    t.tool_pointer,
                    Tool::Pointer,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Pointer))),
                ),
                space().width(4),
                tool_selector(
                    icon::Pencil,
                    t.tool_pencil,
                    Tool::Pencil,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Pencil))),
                ),
                space().width(4),
                tool_selector(
                    icon::Eraser,
                    t.tool_eraser,
                    Tool::Eraser,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Eraser))),
                ),
                space().width(4),
                tool_selector(
                    icon::Curve,
                    t.tool_curve,
                    Tool::Curve,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Curve))),
                ),
                space().width(4),
                tool_button(
                    icon::Quantize,
                    t.tool_quantize,
                    Event::quantize(),
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Quantize))),
                ),
                space().width(4),
                // 变速按钮始终可点击：Ctrl+Click 打开变速对话框不需要选中音符。
                // 普通点击的无选中情况由 handler 内部的 selected.is_empty() 兜底。
                flip_button(
                    icon::Speed,
                    t.tool_speed,
                    Event::speed_change(),
                    true,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Speed))),
                ),
                space().width(4),
                flip_button(
                    icon::FlipVertical,
                    t.tool_flip_vertical,
                    Event::flip_vertical(),
                    has_selection,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::FlipVertical))),
                ),
                space().width(4),
                flip_button(
                    icon::FlipHorizontal,
                    t.tool_flip_horizontal,
                    if self.shift_pressed {
                        Event::flip_horizontal(FlipHorizontalMode::Right)
                    } else if self.ctrl_pressed {
                        Event::flip_horizontal(FlipHorizontalMode::Left)
                    } else {
                        Event::flip_horizontal(FlipHorizontalMode::Center)
                    },
                    has_selection,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::FlipHorizontal))),
                ),
                space().width(8),
                // 分割/合并按钮
                tool_button(
                    icon::Split,
                    t.tool_split,
                    Event::split(),
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Split))),
                ),
                space().width(4),
                tool_button(
                    icon::Glue,
                    t.tool_glue,
                    Event::glue(),
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Glue))),
                ),
                space().width(8),
                // 移调按钮
                // 普通点击 ±1 半音，Ctrl+点击 ±12 半音（一个八度）
                flip_button(
                    icon::TransposeDown,
                    transpose_down_tooltip,
                    transpose_down_event,
                    has_selection,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::TransposeDown))),
                ),
                space().width(4),
                flip_button(
                    icon::TransposeUp,
                    transpose_up_tooltip,
                    transpose_up_event,
                    has_selection,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::TransposeUp))),
                ),
                space().width(8),
                // 连奏按钮
                tool_button(
                    icon::Tie,
                    t.tool_tie,
                    Event::tie(),
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Tie))),
                ),
                space().width(4),
                self.render_precision_selector(content_height, palette, language, t),
            ]
            .align_y(Alignment::Center),
        )
        .width(iced_widget::core::Length::Shrink)
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
        })
        .into()
    }

    /// 渲染工程走带视图专用的工具选择区域
    ///
    /// 仅开启 yinhe 工程走带面板支持的工具：选择/铅笔/曲线/切割/橡皮擦。
    fn render_arrangement_tools_section<'a>(
        &'a self,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        _has_selection: bool,
        t: &'static MainTranslations,
        window: &'a window::Window,
    ) -> Element<'a> {
        container(
            row![
                tool_selector(
                    icon::MousePointer,
                    t.tool_pointer,
                    Tool::Pointer,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Pointer))),
                ),
                space().width(4),
                tool_selector(
                    icon::Curve,
                    t.tool_curve,
                    Tool::Curve,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Curve))),
                ),
                space().width(4),
                tool_selector(
                    icon::Eraser,
                    t.tool_eraser,
                    Tool::Eraser,
                    self.current_tool,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Eraser))),
                ),
                space().width(4),
                flip_button(
                    icon::Speed,
                    t.tool_speed,
                    Event::speed_change(),
                    true,
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Speed))),
                ),
            ]
            .align_y(Alignment::Center),
        )
        .width(iced_widget::core::Length::Shrink)
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
        })
        .into()
    }

    /// 渲染调整大小手柄（底部可拖拽区域）
    pub fn render_resize_handle<'a>(
        &'a self,
        palette: &'a iced_core::theme::palette::Extended,
    ) -> Element<'a> {
        mouse_area(
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
        .into()
    }

    /// 渲染撤销/重做按钮区域，64px 宽
    pub fn render_undo_redo_controls<'a>(
        &'a self,
        content_height: f32,
        palette: &'a iced_core::theme::palette::Extended,
        t: &'static MainTranslations,
        window: &'a window::Window,
    ) -> Element<'a> {
        container(
            row![
                tool_button(
                    icon::Undo,
                    t.undo,
                    Event::undo(),
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Undo))),
                ),
                space().width(4),
                tool_button(
                    icon::Redo,
                    t.redo,
                    Event::redo(),
                    window,
                    Some(Event::button_hovered(Some(ButtonId::Redo))),
                ),
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
        })
        .into()
    }
}
