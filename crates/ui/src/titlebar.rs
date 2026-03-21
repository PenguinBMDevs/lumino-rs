mod logo;
mod menu;
mod traffic;

use iced_core::{Alignment, Length};
use iced_widget::{container, mouse_area, row, space, text};

use super::Element;
use crate::{Theme, window};

pub struct Titlebar;

impl Titlebar {
    pub fn new() -> Self {
        Self
    }

    pub fn view<'a>(
        &'a self,
        window: &'a window::Window,
        use_native_titlebar: bool,
    ) -> Element<'a> {
        // 如果使用经典系统标题栏，只显示菜单（在最左侧）
        if use_native_titlebar {
            return self.view_native_titlebar(window);
        }

        // 自定义标题栏模式
        let mut row = if cfg!(target_os = "macos") {
            row![]
        } else {
            row![logo::view(window), menu::view()]
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

    /// 经典系统标题栏模式：只显示菜单，在最左侧
    fn view_native_titlebar<'a>(&'a self, window: &'a window::Window) -> Element<'a> {
        // 菜单在最左侧，没有 logo 和窗口控制按钮
        let row = row![menu::view()];

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

        // 不需要拖动和双击最大化（由系统标题栏处理）
        inner.into()
    }
}
