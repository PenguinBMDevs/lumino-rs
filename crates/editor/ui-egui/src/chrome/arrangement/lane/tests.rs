//! 走带泳道交互适配的单测。
//!
//! 从 `lane.rs` 拆出（生产代码 + 测试合计控制在 400 行以内），
//! `use super::*` 直接复用父模块的导入与私有项。

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
