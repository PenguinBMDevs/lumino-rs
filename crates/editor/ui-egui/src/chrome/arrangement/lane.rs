//! 走带泳道交互的 egui 适配（**同一份 handler，零 iced 依赖**）。
//!
//! 交互逻辑在 `lumino-arrangement-core`：这里只做「egui 输入 → `ArrangementEvent`」
//! 的转换，并调用同一个 `handle_event`，产出的 `ArrangementAction` 交给生产侧映射。
//!
//! 与 iced 侧 `ArrangementClickCanvas` 完全等价——两边都只是事件适配器，
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
/// `state` 由调用方按帧保存在 egui ctx-data 中（对应 iced 侧的 `Program::State`）。
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
pub fn moved() -> ArrangementEvent {
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
    fn egui_can_drive_the_shared_handler_without_iced() {
        // 这条测试本身就是证据：`lumino-ui-egui`（0 iced 依赖）能直接跑同一份
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
        // 点击路径：按下建立候选，**释放**时才设光标/清选择（与 iced 一致）。
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
            moved(),
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
