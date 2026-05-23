use iced_core::{Alignment, Background, Border, Color, Length};
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

pub fn view(theme: &Theme, _current_mode: AppMode, progress: f32) -> Element<'_> {
    let p = progress.clamp(0.0, 1.0);
    let palette = theme.extended_palette();

    let is_waterfall = p >= 0.5;

    let icon_type = if is_waterfall {
        icon::Icon::Keys
    } else {
        icon::Icon::PencilOutline
    };
    let label = if is_waterfall { "瀑布流" } else { "编辑器" };

    let icon_bg = palette.background.strong.color;
    let text_color = palette.background.neutral.text;

    let icon_el = container(icon::view_with_size_and_theme(icon_type, 13, 13, Some(theme)))
        .width(Length::Fixed(17.0))
        .height(Length::Fixed(17.0))
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(icon_bg)),
            ..Default::default()
        });

    let text_alpha = if is_waterfall { p } else { 1.0 - p };

    let label_text = text(label)
        .size(12)
        .style(move |_theme: &Theme| text::Style {
            color: Some(Color::from_rgba8(
                (text_color.r * 255.0) as u8,
                (text_color.g * 255.0) as u8,
                (text_color.b * 255.0) as u8,
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

    let inner_bg = palette.background.weaker.color;
    let outer_bg = palette.background.weak.color;
    let border_radius = 4.0;

    let inner = container(content)
        .padding([2, 5])
        .style(move |_theme: &Theme| container::Style {
            background: Some(Background::Color(inner_bg)),
            ..Default::default()
        });

    button(inner)
        .padding(2)
        .style(move |_theme: &Theme, status| {
            let bg = match status {
                button::Status::Hovered => outer_bg,
                button::Status::Pressed => palette.background.base.color,
                _ => outer_bg,
            };
            button::Style {
                background: Some(Background::Color(bg)),
                border: Border {
                    radius: border_radius.into(),
                    width: 0.0,
                    color: Color::TRANSPARENT,
                },
                text_color,
                ..Default::default()
            }
        })
        .on_press(Message::ModeToggled)
        .width(70)
        .height(25)
        .into()
}
