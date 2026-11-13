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

    // ── 滚轮：Ctrl 缩放 / Shift 横向 / 普通纵向 ──
    if inside && let Some(delta) = wheel_delta(ui) {
        let (dx, dy) = match delta {
            ArrangementScroll::Lines { x, y } => (x * 30.0, y * 30.0),
            ArrangementScroll::Pixels { x, y } => (x, y),
        };
        if ui.input(|i| i.modifiers.ctrl) {
            let lines = match delta {
                ArrangementScroll::Lines { y, .. } => y,
                ArrangementScroll::Pixels { y, .. } => y / 50.0,
            }
            .clamp(-1.0, 1.0);
            if lines.abs() > f32::EPSILON {
                actions.push(ArrangementAction::ZoomX {
                    zoom: zoom_x * (1.0 + lines * 0.1),
                    fixed_ratio: (local.x / rect.width.max(1.0)).clamp(0.0, 1.0),
                });
                return actions;
            }
        } else if ui.input(|i| i.modifiers.shift) {
            actions.push(ArrangementAction::ScrollX((ruler.scroll_x + dx).max(0.0)));
            return actions;
        } else {
            actions.push(ArrangementAction::ScrollY(
                (track_list.scroll_y - dy).max(0.0),
            ));
            return actions;
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

/// 本帧滚轮增量（Line 直接给刻度，Point 换算成刻度/像素两用）。
fn wheel_delta(ui: &egui::Ui) -> Option<ArrangementScroll> {
    let mut out: Option<ArrangementScroll> = None;
    ui.input(|i| {
        for event in &i.raw.events {
            if let egui::Event::MouseWheel { unit, delta, .. } = event {
                out = Some(match unit {
                    egui::MouseWheelUnit::Line => ArrangementScroll::Lines {
                        x: delta.x,
                        y: delta.y,
                    },
                    egui::MouseWheelUnit::Point => ArrangementScroll::Pixels {
                        x: delta.x,
                        y: delta.y,
                    },
                    egui::MouseWheelUnit::Page => ArrangementScroll::Lines {
                        x: delta.x * 10.0,
                        y: delta.y * 10.0,
                    },
                });
            }
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lane() -> LaneContext<'static> {
        LaneContext {
            rect: Rect2::new(100.0, 50.0, 800.0, 400.0),
            tool: Tool::Pointer,
            track_count: 4,
            selection: None,
            selected_notes: &[],
            ppq: 960,
            precision: SnapSpec::default(),
            time_signatures: &[(0, 4, 4)],
            ctrl: false,
            shift: false,
        }
    }

    #[test]
    fn egui_can_drive_the_shared_handler_without_legacy() {
        // 这条测试本身就是证据：`lumino-ui-egui`（0 旧版 依赖）能直接跑同一份
        // 走带交互逻辑，不必复制选择/框选实现。
        let mut state = ArrangementInteractionState::default();
        let mut viewport = ArrangementViewport {
            canvas_size: Point2::new(800.0, 400.0),
            ..ArrangementViewport::default()
        };

        let _ = feed(
            &mut state,
            &mut viewport,
            pressed_left(),
            cursor_at(300.0, 150.0),
            &lane(),
        );
        // 点击路径：按下建立候选，**释放**时才设光标/清选择（与 旧版 一致）。
        let actions = feed(
            &mut state,
            &mut viewport,
            released_left(),
            cursor_at(300.0, 150.0),
            &lane(),
        );

        // 空白处点击：清空选择 + 设光标（+ 可能选中音轨）。
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, ArrangementAction::CursorSet(_))),
            "应设置播放光标，实际 {:?}",
            actions
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, ArrangementAction::SelectionCleared)),
            "点击应清空选择，实际 {:?}",
            actions
        );
    }

    fn viewport() -> ArrangementViewport {
        ArrangementViewport {
            zoom_x: 0.05,
            canvas_size: Point2::new(800.0, 400.0),
            canvas_offset: Point2::new(0.0, 0.0),
            ..ArrangementViewport::default()
        }
    }

    /// 选区落在屏幕上的矩形（tick/track → 像素）。
    fn selection_pixels(vp: &ArrangementViewport, sel: (f64, f64, usize, usize)) -> Rect2 {
        let (ts, te, lo, hi) = sel;
        let lh = vp.lane_height();
        Rect2::new(
            vp.tick_to_x(ts) - vp.scroll_x,
            lo as f32 * lh - vp.scroll_y,
            (vp.tick_to_x(te) - vp.tick_to_x(ts)).max(1.0),
            ((hi + 1) as f32 * lh - lo as f32 * lh).max(1.0),
        )
    }

    #[test]
    fn move_selection_enters_move_drag() {
        // 已提交选区内按下 → 进入「移动选区」状态（`move_drag`），
        // 证明 `selection` 真的传到了共享 handler。
        let sel = (1920.0, 5760.0, 0, 0);
        let mut vp = viewport();
        let px = selection_pixels(&vp, sel);
        let mut lane = lane();
        lane.selection = Some(sel);
        lane.track_count = 4;
        let mut state = ArrangementInteractionState::default();

        // 选区像素是泳道局部坐标，喂给 handler 的必须是屏幕坐标（加 rect 原点）。
        let origin = Point2::new(lane.rect.x, lane.rect.y);
        let start = origin + Point2::new(px.x + px.width / 2.0, px.y + px.height / 2.0);
        let _ = feed(
            &mut state,
            &mut vp,
            pressed_left(),
            cursor_at(start.x, start.y),
            &lane,
        );

        assert!(
            state.move_drag.is_some(),
            "在选区内按下应进入 move_drag（说明 selection 已传入）"
        );
    }

    #[test]
    fn eraser_press_enters_eraser_drag() {
        // 橡皮工具按下 → `eraser_drag` 被置位（证明 `tool` 真的传到了 handler）。
        let mut vp = viewport();
        let mut lane = lane();
        lane.tool = Tool::Eraser;
        let mut state = ArrangementInteractionState::default();

        let _ = feed(
            &mut state,
            &mut vp,
            pressed_left(),
            cursor_at(120.0, 60.0),
            &lane,
        );
        assert!(
            state.eraser_drag.is_some(),
            "橡皮按下应进入 eraser_drag（说明 tool 已传入）"
        );

        // 移动应产出框选预览（擦除范围可视化）。
        let actions = feed(
            &mut state,
            &mut vp,
            moved_event(),
            cursor_at(320.0, 160.0),
            &lane,
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, ArrangementAction::DragSelectionRect(Some(_)))),
            "橡皮拖动应产出擦除范围预览，实际 {actions:?}"
        );
    }

    #[test]
    fn curve_press_enters_curve_drag() {
        // 曲线工具按下 → `curve_drag` 被置位（证明 `tool` 真的传到了 handler）。
        let mut vp = viewport();
        let mut lane = lane();
        lane.tool = Tool::Curve;
        let mut state = ArrangementInteractionState::default();

        let _ = feed(
            &mut state,
            &mut vp,
            pressed_left(),
            cursor_at(120.0, 60.0),
            &lane,
        );
        assert!(
            state.curve_drag.is_some(),
            "曲线按下应进入 curve_drag（说明 tool 已传入）"
        );
    }

    #[test]
    fn drag_produces_rubber_band_actions() {
        let mut state = ArrangementInteractionState::default();
        let mut viewport = ArrangementViewport {
            canvas_size: Point2::new(800.0, 400.0),
            ..ArrangementViewport::default()
        };

        let _ = feed(
            &mut state,
            &mut viewport,
            pressed_left(),
            cursor_at(200.0, 100.0),
            &lane(),
        );
        let actions = feed(
            &mut state,
            &mut viewport,
            moved_event(),
            cursor_at(500.0, 250.0),
            &lane(),
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, ArrangementAction::DragSelectionRect(Some(_)))),
            "拖拽应产出框选矩形，实际 {:?}",
            actions
        );

        let actions = feed(
            &mut state,
            &mut viewport,
            released_left(),
            cursor_at(500.0, 250.0),
            &lane(),
        );
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, ArrangementAction::SelectionChanged(Some(_)))),
            "释放应提交选择，实际 {:?}",
            actions
        );
    }
}
