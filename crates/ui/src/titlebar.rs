mod logo;
mod menu;
mod traffic;

use iced_core::{Alignment, Length};
use iced_widget::{container, mouse_area, row, text, space};

use super::Element;
use crate::{Theme, window};

pub struct Titlebar {}

impl Titlebar {
    pub fn new() -> Self {
        Self {}
    }

    pub fn view<'a>(&'a self, window: &'a window::Window) -> Element<'a> {
        let mut row = if cfg!(target_os = "macos") {
            row![]
        } else {
            row![logo::view(), menu::view()]
        };

        // Debug 模式下显示 FPS
        if let Some(fps) = window.fps {
            row = row.push(
                container(
                    text(format!("FPS: {:.1}", fps))
                        .size(12)
                        .style(|theme: &Theme| {
                            let palette = theme.extended_palette();
                            text::Style {
                                color: Some(palette.primary.strong.color),
                            }
                        }),
                )
                .padding([0, 10])
                .align_y(iced_core::Alignment::Center)
                .height(Length::Fill),
            );
        }

        if !cfg!(target_os = "macos") {
            row = row.push(space().width(Length::Fill));
            row = row.push(traffic::view(window));
        }

        let inner = container(row)
            .width(Length::Fill)
            .height(30)
            .style(|theme: &Theme| {
                let palette = theme.extended_palette();
                container::Style::default().background(if window.is_focused {
                    palette.background.neutral.color
                } else {
                    palette.background.weaker.color
                })
            })
            .align_y(Alignment::Start);

        mouse_area(inner)
            .on_press(window::Event::drag())
            .on_double_click(window::Event::toggle_maximize())
            .into()
    }
}
