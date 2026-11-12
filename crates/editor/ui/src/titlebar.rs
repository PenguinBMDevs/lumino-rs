//! 标题栏 iced 占位。
//!
//! 主窗口与对话框窗口的标题栏（菜单标签 / logo / 拖动 / 双击最大化 / 红绿灯）
//! 已全部由 egui 绘制并接管（`chrome::titlebar` + `chrome::popup_titlebar`）。
//! iced 侧只保留**同高 30px、同底色**的占位，保证下方内容区起点与布局不变。
//!
//! `menu` 仍提供菜单数据（`MenuConfig`/`menus`/`display_name`）与同宽占位；
//! `mode_toggle` 只提供 `AppMode` 与切换动画状态，不含标题栏渲染。

pub mod menu;
pub mod mode_toggle;

use iced_core::{Alignment, Length};
use iced_widget::{Space, container};

use super::Element;
use crate::titlebar::mode_toggle::AppMode;
use crate::{Theme, window};
use lumino_extras::i18n::Language;

/// 编辑器标题栏
pub struct Titlebar;

impl Default for Titlebar {
    fn default() -> Self {
        Self::new()
    }
}

impl Titlebar {
    /// 创建一个标题栏实例
    pub fn new() -> Self {
        Self
    }

    /// 渲染标题栏占位（30px，保留聚焦态底色）。
    ///
    /// 参数保留是为了不动两处调用点；视觉与命中全部由 egui 接管。
    #[allow(clippy::too_many_arguments)]
    pub fn view<'a>(
        &'a self,
        window: &'a window::Window,
        _use_native_titlebar: bool,
        _current_mode: AppMode,
        _toggle_progress: f32,
        _language: Language,
        _export_material_enabled: bool,
    ) -> Element<'a> {
        bar(window)
    }

    /// 渲染弹窗专用的自制标题栏占位（30px，拖动 / 红绿灯由 egui 接管）。
    pub fn view_popup<'a>(&'a self, window: &'a window::Window) -> Element<'a> {
        bar(window)
    }
}

/// 30px 占位条：底色跟随聚焦态，无子控件、不吃输入。
fn bar<'a>(window: &'a window::Window) -> Element<'a> {
    container(Space::new())
        .width(Length::Fill)
        .height(Length::Fixed(30.0))
        .align_y(Alignment::Center)
        .style(|theme: &Theme| {
            let palette = theme.extended_palette();
            container::Style::default().background(if window.is_focused {
                palette.background.neutral.color
            } else {
                palette.background.weaker.color
            })
        })
        .into()
}
