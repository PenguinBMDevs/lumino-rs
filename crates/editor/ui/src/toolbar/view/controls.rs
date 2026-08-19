//! 工具栏控件渲染函数
//!
//! 包含播放控制、循环按钮、调整手柄、撤销/重做等控件渲染。
//! 工具选择区域（指针/铅笔/橡皮/曲线/颜料桶等）在 `view/tools.rs`。

use iced_core::Alignment;
use iced_widget::{button, container, mouse_area, row, space};

use crate::resources::icon;
use crate::toolbar::buttons::tool_button;
use crate::toolbar::{ButtonId, Event, RESIZE_HANDLE_HEIGHT, Toolbar};
use crate::widget;
use crate::{Element, Message, Theme, window};
use lumino_extras::i18n::MainTranslations;

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
}

impl Toolbar {
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

    /// 渲染撤销/重做按钮区域（固定 64px 宽）
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
