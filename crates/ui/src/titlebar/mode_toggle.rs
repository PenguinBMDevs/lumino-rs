use iced_core::{Alignment, Length};
use iced_widget::{button, container, row, text};

use crate::{Element, Message, Theme, resources::icon};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppMode {
    #[default]
    Editor,
    Waterfall,
}

impl std::fmt::Display for AppMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppMode::Editor => write!(f, "编辑器"),
            AppMode::Waterfall => write!(f, "瀑布流"),
        }
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

pub fn view(_current_mode: AppMode, progress: f32) -> Element<'static> {
    let p = progress.clamp(0.0, 1.0);

    let is_waterfall = p >= 0.5;

    let icon_type = if is_waterfall {
        icon::Icon::Keys
    } else {
        icon::Icon::PencilOutline
    };
    let label = if is_waterfall { "瀑布流" } else { "编辑器" };

    let icon_el = container(icon::view(icon_type))
        .width(Length::Fixed(17.0))
        .height(Length::Fixed(17.0))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(|_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(iced_core::Color::from_rgb(
                0.455, 0.455, 0.455,
            ))),
            ..Default::default()
        });

    let text_alpha = if is_waterfall { p } else { 1.0 - p };
    let text_color_val = if is_waterfall {
        lerp(0.6, 1.0, (p - 0.5) * 2.0)
    } else {
        lerp(1.0, 0.6, p * 2.0)
    };

    let label_text = text(label)
        .size(12)
        .style(move |_theme: &Theme| text::Style {
            color: Some(iced_core::Color::from_rgba8(
                (text_color_val * 255.0) as u8,
                (text_color_val * 255.0) as u8,
                (text_color_val * 255.0) as u8,
                text_alpha,
            )),
        });

    let content = if is_waterfall {
        row![label_text, icon_el]
    } else {
        row![icon_el, label_text]
    }
    .spacing(4)
    .align_y(Alignment::Center);

    let inner = container(content)
        .padding([2, 5])
        .style(|_theme: &Theme| container::Style {
            background: Some(iced_core::Background::Color(iced_core::Color::from_rgb(
                0.749, 0.749, 0.749,
            ))),
            ..Default::default()
        });

    button(inner)
        .padding(2)
        .style(|_theme: &Theme, _status| button::Style {
            background: Some(iced_core::Background::Color(iced_core::Color::from_rgb(
                0.851, 0.851, 0.851,
            ))),
            border: iced_core::Border {
                radius: 4.0.into(),
                width: 0.0,
                color: iced_core::Color::TRANSPARENT,
            },
            ..Default::default()
        })
        .on_press(Message::ModeToggled)
        .width(70)
        .height(25)
        .into()
}
