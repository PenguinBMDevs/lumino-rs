//! 右侧栏 iced 视图：**只剩同宽占位**。
//!
//! 图标列（3 个按钮）/ 面板内容（图片转 MIDI、素材库、钢琴瀑布流）/ 6px 缩放手柄
//! 全部由 egui 绘制并接管输入：
//! - 图标列：`chrome::sidebar::show_right_model`（命中 / 悬停 / tooltip / 动作）
//! - 面板内容：`chrome::right_panel`
//! - 缩放手柄：`chrome::right_panel::show_handle`
//!
//! 本函数只保证 iced 布局留出**与 egui 完全相同的宽度与高度**，让钢琴卷帘区不被
//! 撑开或压缩。任何像素都不再绘制——否则会与 egui 的绘制叠成两层。

use iced_core::Length;
use iced_widget::{Column, Row, Space, container};

use crate::Element;
use crate::right_sidebar::core::{RESIZE_HANDLE_WIDTH, ROUTE_BAR_WIDTH, RightSidebar};

/// 渲染右侧栏占位（面板宽 + 图标列宽）。
pub fn view<'a>(right_sidebar: &'a RightSidebar) -> Element<'a> {
    // 图标列占位：宽度与 egui 绘制的图标列一致。
    let route_bar: Element<'a> = container(
        Column::new()
            .width(ROUTE_BAR_WIDTH)
            .height(Length::Fill)
            .push(Space::new().height(Length::Fill)),
    )
    .width(ROUTE_BAR_WIDTH)
    .height(Length::Fill)
    .into();

    if !right_sidebar.panel_visible {
        return route_bar;
    }

    // 面板占位：手柄 6px + 内容（宽度恒等于 `panel_width - 6`）。
    let panel = Row::new()
        .push(
            container(
                Space::new()
                    .width(Length::Fixed(RESIZE_HANDLE_WIDTH))
                    .height(Length::Fill),
            )
            .height(Length::Fill),
        )
        .push(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill),
        );

    Row::new()
        .push(
            container(panel)
                .width(Length::Fixed(right_sidebar.panel_width))
                .height(Length::Fill),
        )
        .push(route_bar)
        .height(Length::Fill)
        .into()
}
