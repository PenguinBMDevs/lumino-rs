//! 框选框精度测试：上下以单个 key 为标准，左右以用户设置的音符放置精度为标准
//!
//! 背景：`SelectionBoxMode::Direct`（默认模式）下，框选（Selecting）过程
//! 原先 X 方向使用像素级原始 tick（不吸附用户精度）、Y 方向使用像素级
//! `pos.y`（不对齐 key 线）。修改后（Direct/Spring 统一）：
//! - 左右（X/tick）：始终按用户精度（`snap_precision`）吸附
//! - 上下（Y/key）：视觉坐标对齐到单个 key 线（`key_to_y(key)` + `zoom_y` 底边）
//!
//! 测试直接调用 `handle_pointer_pressed` / `handle_moved` / `handle_eraser_pressed`
//! （绕过 `is_inside_canvas` 的 canvas 尺寸检查，与 `pressed_priority.rs` 同模式）。

use crate::EditState;
use crate::Editor;
use crate::tests::test_helpers;
use iced_core::Point;
use lumino_core::storage::config::SelectionBoxMode;
use lumino_message::Tool;

/// 在空白处开始框选（指针工具，无音符命中）
fn start_selection_at(editor: &mut Editor, x: f32, y: f32) {
    let tick = editor.x_to_tick(x);
    let snapped_tick = editor.snap_tick(tick);
    editor.handle_pointer_pressed(Point::new(x, y), None, snapped_tick);
}

/// 从当前 Selecting 状态提取字段
fn selecting_state(editor: &Editor) -> (f32, f32, u16, u16, f32, f32) {
    let EditState::Selecting {
        start_tick,
        current_tick,
        start_key,
        current_key,
        start_y,
        current_y,
    } = editor.editor_state.interaction.edit_state.clone()
    else {
        panic!(
            "当前状态应处于 Selecting，实际为 {:?}",
            editor.editor_state.interaction.edit_state
        );
    };
    (
        start_tick,
        current_tick,
        start_key,
        current_key,
        start_y,
        current_y,
    )
}

// ===== Direct 模式（默认）：左右按用户精度、上下按单个 key =====

#[test]
fn test_pointer_direct_start_snapped_to_precision_and_key() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);
    assert_eq!(
        editor.editor_state.view.selection_box_mode,
        SelectionBoxMode::Direct,
        "Direct 是默认框选框模式，本测试覆盖默认用户路径"
    );

    let view = editor.editor_state.view.clone();
    // 点击空白处：tick=2400（非网格点，snap 后 = 用户精度 1920）；y 在 key 60 中间（像素级）
    let x = view.tick_to_x(2400.0);
    let y = view.key_to_y(60) + view.zoom_y / 2.0;
    start_selection_at(&mut editor, x, y);

    let (start_tick, current_tick, start_key, current_key, start_y, current_y) =
        selecting_state(&editor);
    // 左右：以用户设置的音符放置精度为准，而非像素级 2400
    let snapped_2400 = view.snap_tick(2400.0);
    assert_eq!(
        start_tick, snapped_2400,
        "起点 tick 应吸附到用户精度 {snapped_2400}，而非像素级 2400"
    );
    assert_eq!(current_tick, snapped_2400);
    // 上下：单个 key
    assert_eq!(start_key, 60);
    assert_eq!(current_key, 60);
    assert_eq!(
        start_y,
        view.key_to_y(60),
        "起点 Y 应对齐 key 60 顶线，而非像素 pos.y"
    );
    assert_eq!(
        current_y,
        view.key_to_y(60) + view.zoom_y,
        "终点 Y 应对齐 key 60 底线（顶线 + zoom_y）"
    );
}

#[test]
fn test_pointer_direct_moved_snapped_to_precision_and_key() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);

    let view = editor.editor_state.view.clone();
    start_selection_at(
        &mut editor,
        view.tick_to_x(2400.0),
        view.key_to_y(60) + view.zoom_y / 2.0,
    );

    // 移动到：tick=5000（正向 1/4 提前吸附 → 5760）；key=56（像素级 y 在 key 内偏移）
    let target_x = view.tick_to_x(5000.0);
    let target_y = view.key_to_y(56) + view.zoom_y * 0.3;
    editor.handle_moved(Point::new(target_x, target_y));

    let (_, current_tick, _, current_key, _, current_y) = selecting_state(&editor);
    // 正向拖动：1/4 提前吸附。5000 = 单元 [3840, 5760) 的前 1/4 内 → 吸附到 5760
    //（原 floor 吸附为 3840，需鼠标移动整个精度单元才扩展）
    assert_eq!(
        current_tick, 5760.0,
        "移动中 current_tick 应 1/4 提前吸附到 5760，而非 floor 的 3840"
    );
    assert_eq!(current_key, 56);
    assert_eq!(
        current_y,
        view.key_to_y(56) + view.zoom_y,
        "移动中 current_y 应对齐 key 56 底线，而非像素 pos.y"
    );
}

// ===== 1/4 提前吸附（横向扩展提前） =====

#[test]
fn test_snap_tick_forward_quarter_cell_threshold() {
    // 公式边界验证：默认精度 1920（四分音符），跳变阈值 = 单元前 1/4 = 480
    let editor = Editor::new();
    let view = &editor.editor_state.view;
    assert_eq!(view.snap_precision, 1920.0);

    assert_eq!(view.snap_tick_forward(0.0), 0.0);
    assert_eq!(view.snap_tick_forward(479.0), 0.0, "1/4 前不扩展");
    assert_eq!(
        view.snap_tick_forward(480.0),
        1920.0,
        "1/4 处即扩展一个单元"
    );
    assert_eq!(view.snap_tick_forward(1000.0), 1920.0);
    assert_eq!(view.snap_tick_forward(1919.0), 1920.0, "单元末尾仍在本单元");
    assert_eq!(
        view.snap_tick_forward(1920.0),
        1920.0,
        "网格点本身 = 本单元末尾"
    );
    assert_eq!(
        view.snap_tick_forward(2400.0),
        3840.0,
        "下一单元 1/4 处继续提前扩展"
    );
}

#[test]
fn test_selection_expands_at_quarter_cell() {
    // 框选正向拖动：鼠标进入精度单元的 1/4 处（start + 0.25*I）即扩展，
    // 无需移动整个精度单元（原 floor 行为）
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);

    let view = editor.editor_state.view.clone();
    // 按下在 tick 2200（floor 吸附 → start = 1920），单元 [1920, 3840) 的 1/4 处 = 2400
    start_selection_at(
        &mut editor,
        view.tick_to_x(2200.0),
        view.key_to_y(60) + view.zoom_y / 2.0,
    );
    let (start_tick, _, ..) = selecting_state(&editor);
    assert_eq!(start_tick, 1920.0, "按下位置仍按 floor 吸附");

    // 1/4 处之前（tick 2350）：不扩展，current_tick 仍等于起点（宽度 0）
    editor.handle_moved(Point::new(view.tick_to_x(2350.0), view.key_to_y(60)));
    let (_, current_tick, ..) = selecting_state(&editor);
    assert_eq!(current_tick, 1920.0, "单元 1/4 前不应扩展");

    // 到达 1/4 处（tick 2400）：扩展一个精度单元 → current_tick = 3840
    editor.handle_moved(Point::new(view.tick_to_x(2400.0), view.key_to_y(60)));
    let (_, current_tick, ..) = selecting_state(&editor);
    assert_eq!(
        current_tick, 3840.0,
        "鼠标到达单元 1/4 处即扩展一个精度单元（无需移动整个单元）"
    );
}

#[test]
fn test_selection_reverse_drag_keeps_floor() {
    // 反向拖动（向左）：保持 floor 吸附（跨过网格点才扩展），与修改前一致
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);

    let view = editor.editor_state.view.clone();
    // 按下在 tick 2400（start = 1920），向左拖到 tick 1500（跨过网格点 0）
    start_selection_at(
        &mut editor,
        view.tick_to_x(2400.0),
        view.key_to_y(60) + view.zoom_y / 2.0,
    );
    editor.handle_moved(Point::new(view.tick_to_x(1500.0), view.key_to_y(60)));

    let (_, current_tick, ..) = selecting_state(&editor);
    assert_eq!(
        current_tick, 0.0,
        "反向拖动保持 floor 吸附：鼠标 1500 → 0（跨过网格点才扩展）"
    );
}

// ===== Spring 模式：行为与 Direct 统一（回归） =====

#[test]
fn test_pointer_spring_start_snapped_to_precision_and_key() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);
    editor.editor_state.view.selection_box_mode = SelectionBoxMode::Spring;

    let view = editor.editor_state.view.clone();
    let x = view.tick_to_x(2400.0);
    let y = view.key_to_y(60) + view.zoom_y / 2.0;
    start_selection_at(&mut editor, x, y);

    let (start_tick, _, start_key, _, start_y, current_y) = selecting_state(&editor);
    assert_eq!(
        start_tick,
        view.snap_tick(2400.0),
        "Spring 模式起点 tick 同样按用户精度"
    );
    assert_eq!(start_key, 60);
    assert_eq!(
        start_y,
        view.key_to_y(60),
        "Spring 模式起点 Y 同样对齐 key 线"
    );
    assert_eq!(current_y, view.key_to_y(60) + view.zoom_y);
}

// ===== 橡皮擦 Shift 框选：同样统一精度 =====

#[test]
fn test_eraser_shift_selection_snapped_to_precision_and_key() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);
    editor.editor_state.tool = Tool::Eraser; // 默认 EraserBehavior::Default

    let view = editor.editor_state.view.clone();
    let x = view.tick_to_x(2400.0);
    let y = view.key_to_y(60) + view.zoom_y / 2.0;
    // Shift + 空白处 → 框选删除
    editor.handle_eraser_pressed(Point::new(x, y), true, None);

    let (start_tick, _, start_key, _, start_y, current_y) = selecting_state(&editor);
    assert_eq!(
        start_tick,
        view.snap_tick(2400.0),
        "橡皮擦框选起点 tick 按用户精度"
    );
    assert_eq!(start_key, 60);
    assert_eq!(start_y, view.key_to_y(60), "橡皮擦框选起点 Y 对齐 key 线");
    assert_eq!(current_y, view.key_to_y(60) + view.zoom_y);
}

// ===== Y 向框选工具：Y 全范围行为不受影响（回归） =====

#[test]
fn test_y_select_tool_full_key_range_unchanged() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);
    editor.editor_state.tool = Tool::PointerYSelect;

    let view = editor.editor_state.view.clone();
    let x = view.tick_to_x(2400.0);
    let y = view.key_to_y(60) + view.zoom_y / 2.0;
    start_selection_at(&mut editor, x, y);

    let (start_tick, _, start_key, current_key, start_y, current_y) = selecting_state(&editor);
    // Y 维度自动覆盖全部可见键（0..=127），X 仍按用户精度
    assert_eq!(start_key, 127);
    assert_eq!(current_key, 0);
    assert_eq!(start_y, view.key_to_y(127));
    assert_eq!(current_y, view.key_to_y(0) + view.zoom_y);
    assert_eq!(
        start_tick,
        view.snap_tick(2400.0),
        "Y 向工具 X 维度同样按用户精度"
    );
}
