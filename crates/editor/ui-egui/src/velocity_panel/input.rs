//! 力度面板编辑输入（力度模式 + 曲线工具）。
//!
//! 与 iced `VelocityCanvas` 的力度交互同语义：
//! - 命中力度点（10px）→ 拖拽改力度；
//! - 空白处按下 → 曲线绘制（起点到当前点的线性插值，按下时若有选中集则只影响选中音符）；
//! - 松开 → 结束。
//!
//! Tempo / CC / Bend 的输入仍由 iced Canvas 处理（本模块只在力度模式下占用数据区）。

use egui::{Rect, Vec2};

use super::{VelocityPanelAction, VelocityPanelModel};

/// 命中半径（与 iced `HIT_RADIUS` 一致）。
const HIT_RADIUS: f32 = 10.0;
/// 顶部 resize 手柄高度。
const RESIZE_HANDLE: f32 = 5.0;
/// 面板上下内边距（与 iced `PANEL_PADDING_Y` 一致）。
const PANEL_PADDING_Y: f32 = 12.0;

/// 力度值 → 面板 Y（与 `VelocityCanvas::velocity_to_y` 同公式）。
fn velocity_to_y(velocity: u8, height: f32) -> f32 {
    let min_y = PANEL_PADDING_Y + RESIZE_HANDLE;
    height - (velocity as f32 / 127.0) * (height - min_y)
}

/// 面板 Y → 力度值（与 `VelocityCanvas::y_to_velocity` 同公式）。
fn y_to_velocity(y: f32, height: f32) -> u8 {
    let min_y = PANEL_PADDING_Y + RESIZE_HANDLE;
    let clamped = y.clamp(min_y, height);
    (((height - clamped) / (height - min_y)) * 127.0)
        .round()
        .clamp(0.0, 127.0) as u8
}

/// BPM → 面板 Y（与 `tempo_bpm_to_y` 同公式）。
pub(super) fn tempo_to_y(bpm: f64, max_bpm: f64, height: f32) -> f32 {
    let min_y = PANEL_PADDING_Y + RESIZE_HANDLE;
    let normalized = ((bpm - 20.0) / (max_bpm - 20.0)) as f32;
    height - normalized * (height - min_y)
}

/// 面板 Y → BPM（与 `VelocityCanvas::y_to_bpm` 同公式）。
fn y_to_bpm(y: f32, height: f32, max_bpm: f64) -> f64 {
    let min_y = PANEL_PADDING_Y + RESIZE_HANDLE;
    let clamped = y.clamp(min_y, height);
    let normalized = (height - clamped) / (height - min_y);
    20.0 + normalized as f64 * (max_bpm - 20.0)
}

/// tick → 吸附网格（floor，与 `ViewState::snap_tick` 一致）。
fn snap_tick(tick: f32, precision: f32) -> f32 {
    if precision <= 0.0 {
        return tick;
    }
    (tick / precision).floor() * precision
}

/// 滚轮 → 双轴动作（Pixels ÷50，与 iced 同口径）。
fn wheel_action(ui: &egui::Ui, rect: Rect) -> Option<VelocityPanelAction> {
    let pos = ui.ctx().input(|i| i.pointer.latest_pos())?;
    if !rect.contains(pos) {
        return None;
    }
    let delta = ui.ctx().input(|i| i.smooth_scroll_delta);
    let (dx, dy) = (delta.x, delta.y);
    if dx.abs() < f32::EPSILON && dy.abs() < f32::EPSILON {
        return None;
    }
    Some(VelocityPanelAction::Wheel {
        delta_x: dx,
        delta_y: dy / 50.0,
    })
}

const STATE_ID: &str = "lumino.velocity-panel.edit-state";
const RESIZE_STATE_ID: &str = "lumino.velocity-panel.resize-state";

/// 面板高度下限（与 iced `VELOCITY_PANEL_MIN_HEIGHT` 一致）。
const PANEL_MIN_HEIGHT: f32 = 60.0;
/// 面板高度上限（与 iced `VELOCITY_PANEL_MAX_HEIGHT` 一致）。
const PANEL_MAX_HEIGHT: f32 = 400.0;

/// 顶部 resize 拖拽状态（`start_height = 0` = 未在拖拽）。
#[derive(Default, Clone, Copy)]
struct ResizeState {
    start_y: f32,
    start_height: f32,
}

/// 数据区顶部 resize 手柄命中矩形（画布顶端 5px）。
pub(super) fn resize_zone(canvas: Rect) -> Rect {
    Rect::from_min_size(canvas.min, Vec2::new(canvas.width(), RESIZE_HANDLE))
}

/// 面板 resize 拖拽：按下记基线，拖动改高度，松开清除。
///
/// 公式与 iced `handle_cursor_moved` 一致：
/// `new_height = start_height + (start_y - y)`，钳制到 60..400，
/// 与当前高度差 > 1px 才发动作（避免每帧抖动）。
pub(super) fn handle_resize(
    ui: &egui::Ui,
    model: &VelocityPanelModel,
) -> Option<VelocityPanelAction> {
    let zone = resize_zone(model.canvas);
    let pos = ui.ctx().input(|i| i.pointer.latest_pos())?;
    let pressed = ui.ctx().input(|i| i.pointer.primary_pressed());
    let released = ui.ctx().input(|i| i.pointer.primary_released());
    let down = ui.ctx().input(|i| i.pointer.primary_down());

    let mut state = ui
        .ctx()
        .data(|d| d.get_temp::<ResizeState>(egui::Id::unique(RESIZE_STATE_ID)))
        .unwrap_or_default();

    if zone.contains(pos) && !down {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }

    if pressed && zone.contains(pos) {
        state = ResizeState {
            start_y: pos.y,
            start_height: model.panel.height(),
        };
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique(RESIZE_STATE_ID), state));
        return None;
    }

    if down && state.start_height > 0.0 {
        let new_height = (state.start_height + (state.start_y - pos.y))
            .clamp(PANEL_MIN_HEIGHT, PANEL_MAX_HEIGHT);
        if (new_height - model.panel.height()).abs() > 1.0 {
            return Some(VelocityPanelAction::Resize(new_height));
        }
    }

    if released && state.start_height > 0.0 {
        ui.ctx().data_mut(|d| {
            d.insert_temp(egui::Id::unique(RESIZE_STATE_ID), ResizeState::default());
        });
    }

    None
}

#[derive(Default, Clone)]
struct EditState {
    drag_index: Option<usize>,
    curve: Option<(f32, u8)>,
    tempo_drag: Option<usize>,
    auto: Option<AutoDrag>,
    /// 本次拖拽是否已发过「记历史」的首个移动。
    auto_first: bool,
}

/// 自动化锚点拖拽状态。
#[derive(Debug, Clone, Copy)]
struct AutoDrag {
    track_idx: u16,
    lane_idx: usize,
    old_tick: u32,
    old_value: u16,
}

/// 处理本帧力度编辑输入。
pub(super) fn handle(ui: &egui::Ui, model: &VelocityPanelModel) -> Option<VelocityPanelAction> {
    if !model.velocity_editing {
        return None;
    }
    if let Some(action) = wheel_action(ui, model.canvas) {
        return Some(action);
    }
    let rect = model.canvas;
    let height = rect.height();
    let pos = ui.ctx().input(|i| i.pointer.latest_pos())?;
    let pressed = ui.ctx().input(|i| i.pointer.primary_pressed());
    let released = ui.ctx().input(|i| i.pointer.primary_released());
    let down = ui.ctx().input(|i| i.pointer.primary_down());

    let local = Vec2::new(pos.x - rect.left(), pos.y - rect.top());
    let in_canvas = rect.contains(pos) && local.y > RESIZE_HANDLE;

    let mut state = ui
        .ctx()
        .data(|d| d.get_temp::<EditState>(egui::Id::unique(STATE_ID)))
        .unwrap_or_default();
    let mut action = None;

    if pressed && in_canvas {
        if let Some(idx) = hit_test(model, local) {
            state.drag_index = Some(idx);
            state.curve = None;
            let point = &model.velocity_points[idx];
            action = Some(VelocityPanelAction::VelocityDragStart(
                point.note_index,
                point.velocity,
            ));
        } else {
            state.curve = Some((local.x, y_to_velocity(local.y, height)));
            action = Some(VelocityPanelAction::VelocityCurveStart);
        }
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique(STATE_ID), state.clone()));
        return action;
    }

    if down {
        if let Some(idx) = state.drag_index {
            let point = model.velocity_points.get(idx)?;
            let new_velocity = y_to_velocity(local.y, height);
            if new_velocity != point.velocity {
                action = Some(VelocityPanelAction::VelocityDragMove(
                    point.note_index,
                    new_velocity,
                ));
            }
        } else if let Some((start_x, start_velocity)) = state.curve {
            let current_velocity = y_to_velocity(local.y, height);
            let min_x = start_x.min(local.x);
            let max_x = start_x.max(local.x);
            let mut updates = Vec::new();
            for point in &model.velocity_points {
                if model.has_selection && !model.selected_notes.contains(&point.note_index) {
                    continue;
                }
                let point_x = point.tick * model.view.zoom_x - model.view.scroll_x
                    + model.view.keyboard_width;
                if point_x < min_x || point_x > max_x {
                    continue;
                }
                let t = if (max_x - min_x).abs() < f32::EPSILON {
                    1.0
                } else {
                    (point_x - min_x) / (max_x - min_x)
                };
                let value = start_velocity as f32 * (1.0 - t) + current_velocity as f32 * t;
                let value = value.round().clamp(0.0, 127.0) as u8;
                if value != point.velocity {
                    updates.push((point.note_index, value));
                }
            }
            if !updates.is_empty() {
                action = Some(VelocityPanelAction::VelocityCurvePaint(updates));
            }
        }
    }

    if released {
        if state.drag_index.take().is_some() {
            action = Some(VelocityPanelAction::VelocityDragEnd);
        } else if state.curve.take().is_some() {
            action = Some(VelocityPanelAction::VelocityCurveEnd);
        }
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique(STATE_ID), state));
    }

    action
}

fn hit_test(model: &VelocityPanelModel, local: Vec2) -> Option<usize> {
    model
        .velocity_points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let x =
                point.tick * model.view.zoom_x - model.view.scroll_x + model.view.keyboard_width;
            let y = velocity_to_y(point.velocity, model.canvas.height());
            let d = ((local.x - x).powi(2) + (local.y - y).powi(2)).sqrt();
            (index, d)
        })
        .filter(|(_, distance)| *distance < HIT_RADIUS)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

/// 处理本帧 Tempo 编辑输入（拖拽 / 新增 / 删除）。
pub(super) fn handle_tempo(
    ui: &egui::Ui,
    model: &VelocityPanelModel,
) -> Option<VelocityPanelAction> {
    if !model.tempo_editing {
        return None;
    }
    if let Some(action) = wheel_action(ui, model.canvas) {
        return Some(action);
    }
    let rect = model.canvas;
    let height = rect.height();
    let pos = ui.ctx().input(|i| i.pointer.latest_pos())?;
    let pressed = ui.ctx().input(|i| i.pointer.primary_pressed());
    let right_clicked = ui.ctx().input(|i| i.pointer.secondary_clicked());
    let released = ui.ctx().input(|i| i.pointer.primary_released());
    let down = ui.ctx().input(|i| i.pointer.primary_down());
    let local = Vec2::new(pos.x - rect.left(), pos.y - rect.top());
    let in_canvas = rect.contains(pos) && local.y > RESIZE_HANDLE;

    let mut state = ui
        .ctx()
        .data(|d| d.get_temp::<EditState>(egui::Id::unique(STATE_ID)))
        .unwrap_or_default();
    let mut action = None;

    if right_clicked && in_canvas {
        if let Some(idx) = hit_test_tempo(model, local) {
            return Some(VelocityPanelAction::TempoDelete(idx));
        }
        return None;
    }

    if pressed && in_canvas {
        if let Some(idx) = hit_test_tempo(model, local) {
            if model.eraser_tool {
                return Some(VelocityPanelAction::TempoDelete(idx));
            }
            state.tempo_drag = Some(idx);
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique(STATE_ID), state));
            return Some(VelocityPanelAction::TempoDragStart(idx));
        }
        if model.eraser_tool {
            return None;
        }
        let tick = snap_tick(
            ((local.x - model.view.keyboard_width + model.view.scroll_x) / model.view.zoom_x)
                .max(0.0),
            model.view.snap_precision,
        );
        let bpm = y_to_bpm(local.y, height, model.tempo_max_bpm).clamp(20.0, model.tempo_max_bpm);
        return Some(VelocityPanelAction::TempoAdd(tick, bpm));
    }

    if down && let Some(idx) = state.tempo_drag {
        let bpm = y_to_bpm(local.y, height, model.tempo_max_bpm).clamp(20.0, model.tempo_max_bpm);
        action = Some(VelocityPanelAction::TempoDragMove(idx, bpm));
    }

    if released && state.tempo_drag.take().is_some() {
        action = Some(VelocityPanelAction::TempoDragEnd);
        ui.ctx()
            .data_mut(|d| d.insert_temp(egui::Id::unique(STATE_ID), state));
    }

    action
}

fn hit_test_tempo(model: &VelocityPanelModel, local: Vec2) -> Option<usize> {
    model
        .tempo_points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            let x =
                point.tick * model.view.zoom_x - model.view.scroll_x + model.view.keyboard_width;
            let y = tempo_to_y(point.bpm, model.tempo_max_bpm, model.canvas.height());
            let d = ((local.x - x).powi(2) + (local.y - y).powi(2)).sqrt();
            (index, d)
        })
        .filter(|(_, distance)| *distance < HIT_RADIUS)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(index, _)| index)
}

mod automation;

pub(super) use automation::handle as handle_automation;

/// 数据区在力度编辑时从 iced 手里接管的矩形（保留顶部 resize 手柄给 iced）。
pub(super) fn blocking_rect(canvas: Rect) -> Rect {
    Rect::from_min_max(
        egui::pos2(canvas.left(), canvas.top() + RESIZE_HANDLE),
        canvas.max,
    )
}
