//! 走带两条滚动条与底部「添加音轨」按钮（egui 接管输入）。
//!
//! 几何与 iced `view_arrangement` 同源：底行 20px 位于状态栏之上，
//! 垂直条 20px 贴右缘、从标尺下方开始。

use egui::{Align2, FontId, Rect};

use super::{ArrangementAction, ArrangementModel, H_SCROLLBAR_H, RULER_HEIGHT, TRACK_LIST_W};
use crate::theme;

/// 取值为 `None`（尚未拿到布局）时整体跳过。
pub(super) fn show(
    ui: &egui::Ui,
    full: Rect,
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
    model: &ArrangementModel,
) -> Option<ArrangementAction> {
    let bars = model.scrollbars?;
    let mut action = None;

    let vertical = crate::scrollbar::ScrollbarModel {
        rect: Rect::from_min_max(
            egui::pos2(right, top + RULER_HEIGHT),
            egui::pos2(full.right(), bottom.max(top + RULER_HEIGHT)),
        ),
        scroll: bars.scroll_y,
        max_scroll: bars.max_scroll_y,
        zoom: bars.zoom_y,
        viewport: Some(bars.viewport_y),
        thumb_ratio: None,
        vertical: true,
    };
    if let Some(scroll) = crate::scrollbar::show_raw(ui, "arrangement-y", &vertical) {
        action = Some(match scroll {
            crate::scrollbar::ScrollbarAction::Scroll(y) => ArrangementAction::ScrollY(y),
            crate::scrollbar::ScrollbarAction::Zoom { zoom, fixed_ratio } => {
                ArrangementAction::ZoomY { zoom, fixed_ratio }
            }
        });
    }

    // 底部行（添加音轨 + 水平滚动条）高 20px，位于状态栏之上；两者都不能
    // 覆盖状态栏，否则会挡住状态栏文字并抢走它下方的点击。
    let row_bottom = (bottom + H_SCROLLBAR_H).min(full.bottom());
    let horizontal = crate::scrollbar::ScrollbarModel {
        rect: Rect::from_min_max(
            egui::pos2(left + TRACK_LIST_W, bottom),
            egui::pos2(right.max(left + TRACK_LIST_W), row_bottom),
        ),
        scroll: bars.scroll_x,
        max_scroll: bars.max_scroll_x,
        zoom: bars.zoom_x,
        viewport: Some(bars.viewport_x),
        thumb_ratio: None,
        vertical: false,
    };
    if let Some(scroll) = crate::scrollbar::show_raw(ui, "arrangement-x", &horizontal) {
        action = Some(match scroll {
            crate::scrollbar::ScrollbarAction::Scroll(x) => ArrangementAction::ScrollX(x),
            crate::scrollbar::ScrollbarAction::Zoom { zoom, fixed_ratio } => {
                ArrangementAction::ZoomX { zoom, fixed_ratio }
            }
        });
    }

    if let Some(add) = add_track(ui, left, bottom, row_bottom) {
        action = Some(add);
    }
    super::super::menu::extend_blocked(ui.ctx(), [vertical.rect, horizontal.rect]);
    action
}

/// 底部左侧「+」按钮（160×20，与 iced 同尺寸）。
///
/// 用裸指针判定：该矩形紧贴状态栏与水平滚动条，`ui.interact` 会被后注册的
/// 面板在命中测试里抢先（真机实测点击无反应）。
fn add_track(ui: &egui::Ui, left: f32, bottom: f32, row_bottom: f32) -> Option<ArrangementAction> {
    let add = Rect::from_min_max(
        egui::pos2(left, bottom),
        egui::pos2(left + TRACK_LIST_W, row_bottom),
    );
    let pointer = ui.ctx().input(|i| i.pointer.interact_pos());
    let pressed = ui.ctx().input(|i| i.pointer.primary_pressed());
    let released = ui.ctx().input(|i| i.pointer.primary_released());
    let press_id = egui::Id::unique("lumino.arrangement.add-track.press");
    let mut armed: bool = ui.ctx().data(|d| d.get_temp(press_id)).unwrap_or(false);
    if pressed && pointer.is_some_and(|p| add.contains(p)) {
        armed = true;
        ui.ctx().data_mut(|d| d.insert_temp(press_id, armed));
    }
    let mut action = None;
    if released {
        if armed && pointer.is_some_and(|p| add.contains(p)) {
            action = Some(ArrangementAction::AddTrack);
        }
        armed = false;
        ui.ctx().data_mut(|d| d.insert_temp(press_id, armed));
    }
    let hovered = pointer.is_some_and(|p| add.contains(p));
    ui.painter().rect_filled(
        add,
        0,
        if hovered {
            theme::hover_bg()
        } else {
            theme::base_bg()
        },
    );
    ui.painter().text(
        add.center(),
        Align2::CENTER_CENTER,
        "+",
        FontId::proportional(14.0),
        theme::text(),
    );
    super::super::menu::extend_blocked(ui.ctx(), [add]);
    action
}
