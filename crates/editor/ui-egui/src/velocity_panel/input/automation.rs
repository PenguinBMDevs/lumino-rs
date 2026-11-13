//! CC / Bend 自动化编辑输入。
//!
//! 与 旧版 `handle_automation_*` 同语义（Curve 工具编辑、Eraser 删除、
//! 右键删除、空白新增）；拖拽首个移动由生产侧记一次历史，后续走批量。
//! 拆出本文件以保持 `input.rs` < 400 行。

use egui::Vec2;

use super::{EditState, HIT_RADIUS, RESIZE_HANDLE, STATE_ID, snap_tick, wheel_action};
use crate::velocity_panel::{
    AutomationAnchor, AutomationLaneModel, VelocityPanelAction, VelocityPanelModel,
};

/// 自动化值 → 面板 Y（与 gfx `AutomationViewParams::value_to_y` 同公式）。
fn value_to_y(value: f32, max_value: f32, value_zoom: f32, value_scroll: f32, height: f32) -> f32 {
    let visible_range = max_value / value_zoom;
    if visible_range <= 0.0 {
        return height;
    }
    height - ((value - value_scroll) / visible_range) * height
}

/// 面板 Y → 自动化值（与 gfx `AutomationViewParams::y_to_value` 同公式）。
fn y_to_value(y: f32, max_value: f32, value_zoom: f32, value_scroll: f32, height: f32) -> f32 {
    let visible_range = max_value / value_zoom;
    if visible_range <= 0.0 {
        return 0.0;
    }
    value_scroll + (1.0 - (y / height)) * visible_range
}

/// tick → 吸附网格（floor，与 `ViewState::snap_tick` 一致）。
fn snapped_tick(model: &VelocityPanelModel, x: f32) -> u32 {
    snap_tick(
        ((x - model.view.keyboard_width + model.view.scroll_x) / model.view.zoom_x).max(0.0),
        model.view.snap_precision,
    ) as u32
}

/// 处理本帧 CC / Bend 自动化编辑输入。
pub(in crate::velocity_panel) fn handle(
    ui: &egui::Ui,
    model: &VelocityPanelModel,
) -> Option<VelocityPanelAction> {
    if !model.automation_editing {
        return None;
    }
    let lane = model.automation.as_ref()?;
    if let Some(action) = wheel_action(ui, model.canvas) {
        return Some(action);
    }
    let rect = model.canvas;
    let height = rect.height();
    let pos = ui.ctx().input(|i| i.pointer.latest_pos())?;
    let pressed = ui.ctx().input(|i| i.pointer.primary_pressed());
    let released = ui.ctx().input(|i| i.pointer.primary_released());
    let right_clicked = ui.ctx().input(|i| i.pointer.secondary_clicked());
    let down = ui.ctx().input(|i| i.pointer.primary_down());
    let local = Vec2::new(pos.x - rect.left(), pos.y - rect.top());
    let in_canvas = rect.contains(pos) && local.y > RESIZE_HANDLE;

    let mut state = ui
        .ctx()
        .data(|d| d.get_temp::<EditState>(egui::Id::unique(STATE_ID)))
        .unwrap_or_default();
    let mut action = None;

    // 右键：命中锚点即删除。
    if right_clicked && in_canvas {
        if let (Some(lane_idx), Some(anchor)) = (lane.lane_idx, hit_test(lane, local, model)) {
            return Some(VelocityPanelAction::AutomationDelete {
                track_idx: lane.track_idx,
                lane_idx,
                tick: anchor.tick,
            });
        }
        return None;
    }

    if pressed && in_canvas {
        if let (Some(lane_idx), Some(anchor)) = (lane.lane_idx, hit_test(lane, local, model)) {
            if model.eraser_tool {
                return Some(VelocityPanelAction::AutomationDelete {
                    track_idx: lane.track_idx,
                    lane_idx,
                    tick: anchor.tick,
                });
            }
            state.auto = Some(super::AutoDrag {
                track_idx: lane.track_idx,
                lane_idx,
                old_tick: anchor.tick,
                old_value: anchor.value,
            });
            state.auto_first = true;
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique(STATE_ID), state));
            return None;
        }
        if model.eraser_tool {
            return None;
        }
        // 空白处：新增锚点（Curve 工具）。
        let tick = snapped_tick(model, local.x);
        let value = y_to_value(
            local.y,
            lane.max_value,
            model.value_zoom,
            model.value_scroll,
            height,
        );
        return Some(VelocityPanelAction::AutomationAdd {
            track_idx: lane.track_idx,
            controller: lane.controller,
            tick,
            value: value.round().clamp(0.0, lane.max_value) as u16,
        });
    }

    if down && let Some(drag) = state.auto {
        let tick = snapped_tick(model, local.x);
        let value = y_to_value(
            local.y,
            lane.max_value,
            model.value_zoom,
            model.value_scroll,
            height,
        );
        let new_value = value.round().clamp(0.0, lane.max_value) as u16;
        if new_value != drag.old_value || tick != drag.old_tick {
            action = Some(VelocityPanelAction::AutomationMove {
                track_idx: drag.track_idx,
                lane_idx: drag.lane_idx,
                old_tick: drag.old_tick,
                old_value: lane.controller.is_none().then_some(drag.old_value),
                new_tick: tick,
                new_value,
                first: state.auto_first,
            });
            state.auto_first = false;
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique(STATE_ID), state.clone()));
        }
    }

    if released {
        state.auto = None;
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique(STATE_ID), state));
    }

    action
}

fn hit_test(
    lane: &AutomationLaneModel,
    local: Vec2,
    model: &VelocityPanelModel,
) -> Option<AutomationAnchor> {
    lane.anchors
        .iter()
        .map(|anchor| {
            let x = anchor.tick as f32 * model.view.zoom_x - model.view.scroll_x
                + model.view.keyboard_width;
            let y = value_to_y(
                anchor.value as f32,
                lane.max_value,
                model.value_zoom,
                model.value_scroll,
                model.canvas.height(),
            );
            let d = ((local.x - x).powi(2) + (local.y - y).powi(2)).sqrt();
            (*anchor, d)
        })
        .filter(|(_, distance)| *distance < HIT_RADIUS)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(anchor, _)| anchor)
}
