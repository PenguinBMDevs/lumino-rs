//! 录制按钮渲染

use crate::toolbar::{Event, Toolbar};
use crate::{Element, Theme, window};
use iced_widget::{button, container, text};

impl Toolbar {
    /// 渲染录制按钮
    pub fn render_record_button<'a>(
        &'a self,
        content_height: f32,
        _palette: &iced_core::theme::palette::Extended,
        _window: &'a window::Window,
    ) -> Element<'a> {
        let is_recording = self.is_recording;
        let weak_color = _palette.background.weak.color;
        let strong_color = _palette.background.strong.color;
        let (bg_color, text_color) = if is_recording {
            (
                iced_core::Color::from_rgb(0.8, 0.1, 0.1),
                iced_core::Color::WHITE,
            )
        } else {
            (weak_color, iced_core::Color::from_rgb(0.8, 0.1, 0.1))
        };

        let label = if is_recording { "● REC" } else { "●" };

        let on_press = if is_recording {
            Event::record_stop()
        } else {
            Event::record()
        };

        container(
            button(
                container(text(label).size(16).color(text_color).center())
                    .width(iced_widget::core::Length::Fill)
                    .height(iced_widget::core::Length::Fill)
                    .align_x(iced_core::alignment::Horizontal::Center)
                    .align_y(iced_core::alignment::Vertical::Center),
            )
            .on_press(on_press)
            .style(move |_theme: &Theme, status| {
                let bg = if status == iced_widget::button::Status::Hovered {
                    if is_recording {
                        iced_core::Color::from_rgb(0.9, 0.2, 0.2)
                    } else {
                        strong_color
                    }
                } else {
                    bg_color
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
            .width(iced_widget::core::Length::Fill)
            .height(iced_widget::core::Length::Fill)
            .padding(4),
        )
        .width(56)
        .height(content_height)
        .align_y(iced_core::alignment::Vertical::Center)
        .align_x(iced_core::alignment::Horizontal::Center)
        .style(move |_theme: &Theme| {
            container::Style::default()
                .background(iced_core::Color::TRANSPARENT)
                .border(iced_core::Border {
                    radius: 4.0.into(),
                    width: 0.0,
                    color: iced_core::Color::TRANSPARENT,
                })
        })
        .into()
    }
}
