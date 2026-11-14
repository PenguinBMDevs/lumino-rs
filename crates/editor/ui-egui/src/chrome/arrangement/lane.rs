//! 走带泳道交互的 egui 适配（**同一份 handler，零 旧版 依赖**）。
//!
//! 交互逻辑在 `lumino-arrangement-core`：这里只做「egui 输入 → `ArrangementEvent`」
//! 的转换，并调用同一个 `handle_event`，产出的 `ArrangementAction` 交给生产侧映射。
//!
//! 与 旧版 侧 `ArrangementClickCanvas` 完全等价——两边都只是事件适配器，
//! 选择/移动/框选/橡皮/曲线行为不会出现两套实现。

use lumino_arrangement_core::interaction::{ArrangementInteractionState, handle_event};
use lumino_arrangement_core::viewport::ArrangementViewport;
use lumino_core::{SnapSpec, Tool};
use lumino_message::arrangement_action::ArrangementAction;
use lumino_message::arrangement_event::{
    ArrangementCursor, ArrangementEvent, ArrangementPointerButton, ArrangementScroll,
};
use lumino_message::types::geometry::{Point2, Rect2};

/// 一次交互所需的只读上下文（几何 + 工具 + 选择快照）。
pub struct LaneContext<'a> {
    /// 泳道区在屏幕上的矩形（来自 `arrangement_geometry` 的 offset/size）。
    pub rect: Rect2,
    /// 当前工具。
    pub tool: Tool,
    /// 总音轨数。
    pub track_count: usize,
    /// 当前已提交的选择矩形。
    pub selection: Option<(f64, f64, usize, usize)>,
    /// 选中音符快照（移动时用于 ghost 预览）。
    pub selected_notes: &'a [(f64, f64, usize, u8)],
    /// 每四分音符 tick 数。
    pub ppq: u16,
    /// 吸附精度。
    pub precision: SnapSpec,
    /// 拍号变化表。
    pub time_signatures: &'a [(u32, u8, u8)],
    /// Ctrl 是否按下。
    pub ctrl: bool,
    /// Shift 是否按下。
    pub shift: bool,
}

/// 把一次 egui 输入喂给共享 handler，返回本帧动作。
///
/// `state` 由调用方按帧保存在 egui ctx-data 中（对应 旧版 侧的 `Program::State`）。
pub fn feed(
    state: &mut ArrangementInteractionState,
    viewport: &mut ArrangementViewport,
    event: ArrangementEvent,
    cursor: ArrangementCursor,
    lane: &LaneContext<'_>,
) -> Vec<ArrangementAction> {
    let ctx = lumino_arrangement_core::interaction::ArrangementInteractionContext {
        event: &event,
        bounds: lane.rect,
        cursor,
        current_tool: lane.tool,
        track_count: lane.track_count,
        arr_sel_rect: lane.selection,
        selected_notes: lane.selected_notes,
        ppq: lane.ppq,
        precision: lane.precision,
        time_signatures: lane.time_signatures,
        ctrl_pressed: lane.ctrl,
        shift_pressed: lane.shift,
    };
    handle_event(state, viewport, &ctx)
}

/// 常用事件的便捷构造（egui 侧调用）。
pub fn pressed_left() -> ArrangementEvent {
    ArrangementEvent::ButtonPressed(ArrangementPointerButton::Left)
}

/// 左键释放。
pub fn released_left() -> ArrangementEvent {
    ArrangementEvent::ButtonReleased(ArrangementPointerButton::Left)
}

/// 指针移动。
pub fn moved_event() -> ArrangementEvent {
    ArrangementEvent::CursorMoved
}

/// 滚轮（按行）。
pub fn wheel_lines(x: f32, y: f32) -> ArrangementEvent {
    ArrangementEvent::WheelScrolled {
        delta: ArrangementScroll::Lines { x, y },
    }
}

/// 屏幕坐标 → 中立光标。
pub fn cursor_at(x: f32, y: f32) -> ArrangementCursor {
    ArrangementCursor::available(Point2::new(x, y))
}

/// 泳道输入状态在 ctx-data 中的键（`Program::State` 的 egui 等价物）。
const STATE_ID: &str = "lumino.arrangement.lane-state";

/// 中键平移状态（视图局部坐标）。
const PAN_ID: &str = "lumino.arrangement.lane-pan";

/// egui 侧驱动：把本帧指针事件喂给共享 handler，返回动作。
///
/// `lane_model` 携带真实输入（工具 / 选区 / 选中音符 / 精度），
/// 滚轮与中键平移在此实现（等价 旧版 `ArrangementClickCanvas` 的导航逻辑）。
pub fn show(
    ui: &egui::Ui,
    lane_model: &crate::chrome::ArrangementLaneModel,
    ruler: &crate::chrome::ArrangementRulerModel,
    track_list: &crate::chrome::ArrangementTrackListModel,
) -> Vec<ArrangementAction> {
    let rect = lane_model.rect;
    let zoom_x = ruler.zoom_x.max(1e-6);
    let zoom_y = (track_list.track_height / lane_model.track_height.max(1e-6)).max(1e-6);
    let mut viewport = ArrangementViewport {
        scroll_x: ruler.scroll_x,
        scroll_y: track_list.scroll_y,
        zoom_x,
        zoom_y,
        track_height: lane_model.track_height,
        canvas_offset: Point2::new(rect.x, rect.y),
        canvas_size: Point2::new(rect.width, rect.height),
        total_ticks: ruler.total_ticks,
        ppq: ruler.ppq,
        ..ArrangementViewport::default()
    };

    let (
        pointer,
        left_pressed,
        left_released,
        middle_pressed,
        middle_released,
        middle_down,
        moving,
    ) = ui.input(|i| {
        (
            i.pointer.interact_pos(),
            i.pointer.any_pressed() && i.pointer.primary_pressed(),
            i.pointer.primary_released(),
            i.pointer.button_pressed(egui::PointerButton::Middle),
            i.pointer.button_released(egui::PointerButton::Middle),
            i.pointer.button_down(egui::PointerButton::Middle),
            i.pointer.is_moving(),
        )
    });
    let Some(pos) = pointer else {
        return Vec::new();
    };
    let local = Point2::new(pos.x - rect.x, pos.y - rect.y);
    let inside = rect.contains(Point2::new(pos.x, pos.y));
    let mut actions = Vec::new();

    // ── 中键平移（等价 旧版 的 middle_drag） ──
    if middle_pressed && inside {
        ui.ctx().data_mut(|d| {
            d.insert_temp(egui::Id::unique(PAN_ID), local);
        });
    } else if middle_down {
        if let Some(start) = ui
            .ctx()
            .data(|d| d.get_temp::<Point2>(egui::Id::unique(PAN_ID)))
        {
            let delta = local - start;
            if delta.x != 0.0 || delta.y != 0.0 {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(egui::Id::unique(PAN_ID), local);
                });
                actions.push(ArrangementAction::ScrollX(
                    (ruler.scroll_x - delta.x).max(0.0),
                ));
                actions.push(ArrangementAction::ScrollY(
                    (track_list.scroll_y - delta.y).max(0.0),
                ));
                return actions;
            }
        }
    } else if middle_released {
        ui.ctx()
            .data_mut(|d| d.remove::<Point2>(egui::Id::unique(PAN_ID)));
    }

    // ── 滚轮：Ctrl 缩放 / Shift 横向 / 普通双轴滚动 ──
    if inside && let Some(wheel) = super::wheel::WheelGesture::collect(ui) {
        if ui.input(|i| i.modifiers.ctrl) {
            let lines = wheel.zoom_lines().clamp(-1.0, 1.0);
            if lines.abs() > f32::EPSILON {
                actions.push(ArrangementAction::ZoomX {
                    zoom: zoom_x * (1.0 + lines * 0.1),
                    fixed_ratio: (local.x / rect.width.max(1.0)).clamp(0.0, 1.0),
                });
                return actions;
            }
        } else {
            // 与钢琴卷帘同口径：触控板水平 / 斜向滑动直接映射双轴；
            // Shift+滚轮在平台未提供横向分量时把纵向增量转成横向；
            // 方向为 `scroll = base - delta`（内容跟随手指）。
            let (scroll_x, scroll_y) = wheel.scroll_targets(
                ui.input(|i| i.modifiers.shift),
                ruler.scroll_x,
                track_list.scroll_y,
            );
            if let Some(x) = scroll_x {
                actions.push(ArrangementAction::ScrollX(x));
            }
            if let Some(y) = scroll_y {
                actions.push(ArrangementAction::ScrollY(y));
            }
            if !actions.is_empty() {
                return actions;
            }
        }
    }

    // ── 工具事件：按下 > 移动 > 释放（一次只喂一个，与 旧版 逐事件投递等价） ──
    let event = if left_pressed && inside {
        Some(pressed_left())
    } else if moving && ui.input(|i| i.pointer.primary_down()) {
        Some(moved_event())
    } else if left_released {
        Some(released_left())
    } else {
        None
    };
    let Some(event) = event else {
        return actions;
    };

    // 交互状态跨帧保存（egui ctx-data 要求 Send+Sync，用 Arc<Mutex>）。
    let state = ui
        .ctx()
        .data(|d| {
            d.get_temp::<std::sync::Arc<std::sync::Mutex<ArrangementInteractionState>>>(
                egui::Id::unique(STATE_ID),
            )
        })
        .unwrap_or_else(|| {
            let state =
                std::sync::Arc::new(std::sync::Mutex::new(ArrangementInteractionState::default()));
            ui.ctx()
                .data_mut(|d| d.insert_temp(egui::Id::unique(STATE_ID), state.clone()));
            state
        });

    let lane_ctx = LaneContext {
        rect,
        tool: lane_model.tool,
        track_count: track_list.tracks.len().max(1),
        selection: lane_model.selection,
        selected_notes: &lane_model.selected_notes,
        ppq: ruler.ppq,
        precision: lane_model.precision,
        time_signatures: &ruler.time_signatures,
        ctrl: ui.input(|i| i.modifiers.ctrl),
        shift: ui.input(|i| i.modifiers.shift),
    };

    let mut state = state.lock().expect("lane state lock");
    actions.extend(feed(
        &mut state,
        &mut viewport,
        event,
        cursor_at(pos.x, pos.y),
        &lane_ctx,
    ));
    actions
}

#[cfg(test)]
mod tests;
