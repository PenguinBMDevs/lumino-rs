//! 对话框窗口的自制标题栏（30px）：拖动区 + 右侧红绿灯。
//!
//! 取代 旧版 `titlebar::view_popup`：对话框内容（各 egui 模块）在
//! `titlebar == true` 时主动让出顶部 30px，本模块正好填满该区域。
//! 只有绘制与命中，动作经 [`TitlebarAction`] 回传生产侧。

use egui::{Align, Layout, Rect};

use super::titlebar::{TitlebarAction, push_titlebar_action};
use crate::icons::{Icon, IconCache};
use crate::layout;
use crate::theme;
use crate::widgets;

/// 对话框标题栏模型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PopupTitlebarModel {
    /// 窗口是否持有焦点（决定底色）。
    pub focused: bool,
}

/// 绘制对话框标题栏（铺满窗口顶部 30px）。
pub fn show(ui: &mut egui::Ui, model: &PopupTitlebarModel, icons: &mut IconCache) {
    let full = ui.ctx().viewport_rect();
    let rect = Rect::from_min_size(full.min, egui::vec2(full.width(), layout::TITLEBAR_H));
    if rect.width() <= 0.0 {
        return;
    }
    ui.painter().rect_filled(
        rect,
        0,
        if model.focused {
            theme::titlebar_bg()
        } else {
            theme::titlebar_bg_unfocused()
        },
    );

    // 右侧三个窗口按钮（与主标题栏同尺寸 45×29、图标 10）。
    let traffic = Rect::from_min_size(
        egui::pos2(rect.right() - layout::TRAFFIC_BTN_W * 3.0, rect.top()),
        egui::vec2(layout::TRAFFIC_BTN_W * 3.0, rect.height()),
    );
    let mut bar = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(traffic)
            .layout(Layout::right_to_left(Align::Center)),
    );
    bar.spacing_mut().item_spacing.x = 0.0;
    let close = widgets::traffic_button(&mut bar, icons, Icon::WindowClose, true);
    let max = widgets::traffic_button(&mut bar, icons, Icon::WindowMax, false);
    let min = widgets::traffic_button(&mut bar, icons, Icon::WindowMin, false);
    if close.clicked() {
        push_titlebar_action(ui.ctx(), TitlebarAction::Close);
    }
    if max.clicked() {
        push_titlebar_action(ui.ctx(), TitlebarAction::ToggleMaximize);
    }
    if min.clicked() {
        push_titlebar_action(ui.ctx(), TitlebarAction::Minimize);
    }

    // 其余区域：按下拖动窗口（对话框不做双击最大化，与 旧版 popup 同行为）。
    let drag = Rect::from_min_max(rect.min, egui::pos2(traffic.left(), rect.bottom()));
    if drag.width() > 0.0 {
        let response = ui.interact(
            drag,
            egui::Id::unique("lumino.popup-titlebar.drag"),
            egui::Sense::click_and_drag(),
        );
        if response.drag_started() {
            push_titlebar_action(ui.ctx(), TitlebarAction::Drag);
        }
    }
}
