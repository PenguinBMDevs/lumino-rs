//! 左侧栏 iced 占位。
//!
//! 路由栏（48px 图标列）、音轨列表、右键菜单、行内重命名、取色器、
//! 端口/通道选择器、拖拽排序、6px 宽度手柄**全部**由 egui 绘制并接管输入
//! （`lumino-ui-egui::chrome::sidebar`）。
//!
//! iced 侧只保留同宽占位（路由栏 48 + 面板 `panel_width`），保证右侧内容区
//! 起始位置与 iced 布局一致；任何像素都不再绘制，否则会与 egui 叠成两层。

use iced_core::Length;
use iced_widget::{Space, container};

use super::core::Sidebar;
use crate::Element;

impl Sidebar {
    /// 渲染左侧栏占位（宽度 = 路由栏 + 可见面板宽）。
    pub fn view(&self) -> Element<'_> {
        container(Space::new())
            .width(Length::Fixed(self.width() as f32))
            .height(Length::Fill)
            .into()
    }

    /// 返回侧边栏占据的宽度（像素）。
    pub fn width(&self) -> u32 {
        (super::core::ROUTE_BAR_WIDTH
            + if self.panel_visible {
                self.panel_width
            } else {
                0.0
            }) as u32
    }
}
