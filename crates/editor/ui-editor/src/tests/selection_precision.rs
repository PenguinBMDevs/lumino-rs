//! 框选框精度测试：上下以单个 key 为标准，左右以鼠标精确 tick 位置为标准
//!
//! 背景：`SelectionBoxMode::Direct`（默认模式）下，框选（Selecting）过程
//! 原先 X 方向使用像素级原始 tick（不吸附用户精度）、Y 方向使用像素级
//! `pos.y`（不对齐 key 线）。修改后（Direct/Spring 统一）：
//! - 左右（X/tick）：始终精确跟随鼠标 tick 位置（像素级，不吸附）。
//!   曾引入吸附（`snap_tick_forward` 1/4 提前吸附 / floor 吸附），导致选框
//!   边界相对鼠标位置多延伸出最多一个精度单元（正向 0.75 单元、反向 1 单元）
//!   且选中鼠标未扫过的音符，属 BUG，已回退为精确跟随。
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

// ===== Direct 模式（默认）：左右精确跟随鼠标 tick、上下对齐单个 key =====

#[test]
fn test_pointer_direct_start_precise_tick_and_key() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);
    assert_eq!(
        editor.editor_state.view.selection_box_mode,
        SelectionBoxMode::Direct,
        "Direct 是默认框选框模式，本测试覆盖默认用户路径"
    );

    let view = editor.editor_state.view.clone();
    // 点击空白处：tick=2400（非网格点，不吸附）；y 在 key 60 中间（像素级）
    let x = view.tick_to_x(2400.0);
    let y = view.key_to_y(60) + view.zoom_y / 2.0;
    start_selection_at(&mut editor, x, y);

    let (start_tick, current_tick, start_key, current_key, start_y, current_y) =
        selecting_state(&editor);
    // 左右：精确跟随鼠标 tick 位置（像素级），不吸附到用户精度
    assert_eq!(
        start_tick, 2400.0,
        "起点 tick 应为鼠标精确位置 2400，而非吸附后的网格点"
    );
    assert_eq!(current_tick, 2400.0);
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
fn test_pointer_direct_moved_precise_tick_and_key() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);

    let view = editor.editor_state.view.clone();
    start_selection_at(
        &mut editor,
        view.tick_to_x(2400.0),
        view.key_to_y(60) + view.zoom_y / 2.0,
    );

    // 移动到：tick=5000（非网格点）；key=56（像素级 y 在 key 内偏移）
    let target_x = view.tick_to_x(5000.0);
    let target_y = view.key_to_y(56) + view.zoom_y * 0.3;
    editor.handle_moved(Point::new(target_x, target_y));

    let (_, current_tick, _, current_key, _, current_y) = selecting_state(&editor);
    // 左右：精确跟随鼠标 tick，选区右边界不越过鼠标位置（不多延伸）
    assert_eq!(
        current_tick, 5000.0,
        "移动中 current_tick 应精确等于鼠标 tick 5000，不吸附、不越界"
    );
    assert_eq!(current_key, 56);
    assert_eq!(
        current_y,
        view.key_to_y(56) + view.zoom_y,
        "移动中 current_y 应对齐 key 56 底线，而非像素 pos.y"
    );
}

// ===== 精确跟随：选框边界永远不越过鼠标位置 =====

#[test]
fn test_selection_follows_mouse_precisely_forward() {
    // 正向（向右）拖动：current_tick 精确跟随鼠标，不多延伸一个精度单元
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);

    let view = editor.editor_state.view.clone();
    // 按下在 tick 2200（精确起点，不再 floor 吸附）
    start_selection_at(
        &mut editor,
        view.tick_to_x(2200.0),
        view.key_to_y(60) + view.zoom_y / 2.0,
    );
    let (start_tick, ..) = selecting_state(&editor);
    assert_eq!(start_tick, 2200.0, "按下位置即精确起点，不吸附");

    // 移动到 tick 2350（仍在单元 [0, 1920) 内）：精确跟随
    editor.handle_moved(Point::new(view.tick_to_x(2350.0), view.key_to_y(60)));
    let (_, current_tick, ..) = selecting_state(&editor);
    assert_eq!(current_tick, 2350.0, "选框右边界应精确在鼠标位置，不多延伸");

    // 移动到 tick 3000（跨过网格点 2880）：仍精确跟随，不吸附到 3840
    editor.handle_moved(Point::new(view.tick_to_x(3000.0), view.key_to_y(60)));
    let (_, current_tick, ..) = selecting_state(&editor);
    assert_eq!(
        current_tick, 3000.0,
        "跨网格点后仍精确跟随鼠标，不 1/4 提前吸附到 3840"
    );
}

#[test]
fn test_selection_follows_mouse_precisely_reverse() {
    // 反向（向左）拖动：current_tick 精确跟随鼠标，不 floor 回拉到单元起点
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);

    let view = editor.editor_state.view.clone();
    // 按下在 tick 2400（精确起点），向左拖到 tick 1500
    start_selection_at(
        &mut editor,
        view.tick_to_x(2400.0),
        view.key_to_y(60) + view.zoom_y / 2.0,
    );
    editor.handle_moved(Point::new(view.tick_to_x(1500.0), view.key_to_y(60)));

    let (start_tick, current_tick, ..) = selecting_state(&editor);
    assert_eq!(
        start_tick, 2400.0,
        "起点 tick 应为鼠标按下位置 2400，而非 floor 吸附的 1920"
    );
    assert_eq!(
        current_tick, 1500.0,
        "反向拖动 current_tick 应精确等于鼠标 tick 1500，向左不多延伸"
    );
}

// ===== Spring 模式：行为与 Direct 统一（回归） =====

#[test]
fn test_pointer_spring_start_precise_tick_and_key() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);
    editor.editor_state.view.selection_box_mode = SelectionBoxMode::Spring;

    let view = editor.editor_state.view.clone();
    let x = view.tick_to_x(2400.0);
    let y = view.key_to_y(60) + view.zoom_y / 2.0;
    start_selection_at(&mut editor, x, y);

    let (start_tick, _, start_key, _, start_y, current_y) = selecting_state(&editor);
    assert_eq!(
        start_tick, 2400.0,
        "Spring 模式起点 tick 同样精确跟随鼠标位置"
    );
    assert_eq!(start_key, 60);
    assert_eq!(
        start_y,
        view.key_to_y(60),
        "Spring 模式起点 Y 同样对齐 key 线"
    );
    assert_eq!(current_y, view.key_to_y(60) + view.zoom_y);
}

// ===== 橡皮擦 Shift 框选：同样精确跟随 =====

#[test]
fn test_eraser_shift_selection_precise_tick_and_key() {
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
        start_tick, 2400.0,
        "橡皮擦框选起点 tick 精确跟随鼠标位置，不吸附"
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
    // Y 维度自动覆盖全部可见键（0..=127），X 仍精确跟随鼠标
    assert_eq!(start_key, 127);
    assert_eq!(current_key, 0);
    assert_eq!(start_y, view.key_to_y(127));
    assert_eq!(current_y, view.key_to_y(0) + view.zoom_y);
    assert_eq!(
        start_tick, 2400.0,
        "Y 向工具 X 维度同样精确跟随鼠标位置，不吸附"
    );
}
