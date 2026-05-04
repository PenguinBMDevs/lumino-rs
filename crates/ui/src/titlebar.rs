mod logo;
pub mod menu;
mod traffic;

use iced_core::{Alignment, Length};
use iced_widget::{container, mouse_area, row, space};

use super::Element;
use crate::{Theme, window};

pub struct Titlebar;

impl Default for Titlebar {
    fn default() -> Self {
        Self::new()
    }
}

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
        // 将菜单区域和可拖动区域分离，避免鼠标事件冲突
        let menu_row = if cfg!(target_os = "macos") {
            row![]
        } else {
            row![logo::view(window), menu::view()]
        };

        // 构建标题栏内容：左侧菜单 + 中间可拖动区域 + 右侧窗口控制

        if cfg!(target_os = "macos") {
            // macOS: 标题栏只保留菜单和弹性空间（FPS 在底部状态栏显示）
            let mut row = menu_row;

            row = row.push(space().width(Length::Fill));

            container(row)
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
                .align_y(Alignment::Start)
                .into()
        } else {
            // Windows/Linux: 左侧菜单（不可拖动）+ 中间可拖动区域 + 右侧窗口控制
            let left_section = container(menu_row)
                .height(Length::Fill)
                .align_y(Alignment::Center);

            // 中间可拖动区域
            let drag_area = mouse_area(container(space().width(Length::Fill)).height(Length::Fill))
                .on_press(window::Event::drag())
                .on_double_click(window::Event::toggle_maximize());

            // 右侧：窗口控制按钮（FPS 已移到底部状态栏显示）
            let right_row = row![traffic::view(window)];

            let right_section = container(right_row)
                .height(Length::Fill)
                .align_y(Alignment::Center);

            container(row![left_section, drag_area, right_section])
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
                .align_y(Alignment::Start)
                .into()
        }
    }

    /// 经典系统标题栏模式：只显示菜单，在最左侧
    fn view_native_titlebar<'a>(&'a self, window: &'a window::Window) -> Element<'a> {
        // 菜单在最左侧，没有 logo 和窗口控制按钮
        let row = if cfg!(target_os = "macos") {
            row![]
        } else {
            row![menu::view()]
        };

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
