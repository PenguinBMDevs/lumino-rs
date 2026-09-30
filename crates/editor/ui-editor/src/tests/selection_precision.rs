//! 框选框精度契约：X 向按「音符精度」单元覆盖式量化，Y 向以单个 key 为标准。
//!
//! ## 契约（2026-08 起）
//!
//! **框选 X 向 = 选中鼠标触碰过的所有 `snap_precision` 单元**（单元覆盖式量化）：
//! - 按下时记录**锚点单元低边** `A = floor(press / p) * p`（`Editor::marquee_anchor_tick`）；
//! - 拖动时两端由 `(A, 鼠标 tick)` 统一推算（`ViewState::snap_marquee_edges`）：
//!   向右拖 → `(A, floor(m) + p)`；向左拖 → `(A + p, floor(m))`；
//! - 因此**两端永远落在格线**上，且方向保持（向左拖时 `start_tick > current_tick`，
//!   供 Spring 弹簧动画按"移动端"驱动）；
//! - 锚点固定 ⇒ 向左拖再拖回右侧时选框能正确**回缩**（否则只能单向扩张）。
//!
//! **Y 向**：起点/终点对齐到单个 key 线（`key_to_y(key)` 顶线、`+zoom_y` 底线）。
//!
//! ## 历史坑（评审勿改回单侧吸附）
//!
//! 本文件曾断言"X 向精确跟随鼠标 tick、不吸附"，因为当时引入的吸附方案是：
//! - `snap_tick_forward`（1/4 提前吸附）→ 移动端**单向外扩**，
//! - 或**单侧** `snap_tick`(floor) 刷两端 → 大端内缩（框内漏选）、小端反向时外扩一格。
//!
//! 两者的共同病根是**两端不同口径**（一端精确 / 一端吸附）。当前方案两端同口径
//! 覆盖「鼠标所在单元」，配合命中半开区间 `[min, max)`（见 `marquee_boundary.rs`），
//! 框边界落格线且贴边音符确定性地不入选。
//!
//! 测试直接调用 `handle_pointer_pressed` / `handle_moved` / `handle_eraser_pressed`
//! （绕过 `is_inside_canvas` 的 canvas 尺寸检查，与 `pressed_priority.rs` 同模式）。

use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use iced_core::Point;
use lumino_core::storage::config::SelectionBoxMode;
use lumino_message::Tool;

/// 默认精度（PPQ 1920 下的 1/4 音符）
const QUARTER: f32 = 1920.0;

/// 在空白处开始框选（指针工具，无音符命中）
fn start_selection_at(editor: &mut Editor, x: f32, y: f32) {
    let tick = editor.x_to_tick(x);
    let snapped_tick = editor.snap_tick(tick);
    editor.handle_pointer_pressed(Point::new(x, y), None, snapped_tick);
}

/// 移动到指定 tick / key 中心
fn move_to(editor: &mut Editor, tick: f32, key: u16) {
    let view = editor.editor_state.view.clone();
    let y = view.key_to_y(key) + view.zoom_y / 2.0;
    editor.handle_moved(Point::new(view.tick_to_x(tick), y));
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

/// 空工程 + 单 key 起点
fn blank_editor() -> Editor {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);
    editor
}

// ===== 单元覆盖式量化：按下 / 移动 =====

#[test]
fn test_pointer_start_covers_anchor_cell() {
    let mut editor = blank_editor();
    assert_eq!(
        editor.editor_state.view.selection_box_mode,
        SelectionBoxMode::Direct,
        "Direct 是默认框选框模式，本测试覆盖默认用户路径"
    );
    assert_eq!(editor.editor_state.view.snap_precision, QUARTER);

    let view = editor.editor_state.view.clone();
    // 按下 tick=2400（落在单元 [1920, 3840) 内）
    let x = view.tick_to_x(2400.0);
    let y = view.key_to_y(60) + view.zoom_y / 2.0;
    start_selection_at(&mut editor, x, y);

    let (start_tick, current_tick, start_key, current_key, start_y, current_y) =
        selecting_state(&editor);
    // X：按下即覆盖锚点所在单元 [1920, 3840)（两端落格线）
    assert_eq!(start_tick, 1920.0, "起点应量化到锚点单元低边（格线）");
    assert_eq!(current_tick, 3840.0, "终点应量化到锚点单元高边（下一格线）");
    // Y：单个 key（行为不变）
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
fn test_pointer_moved_covers_mouse_cell() {
    let mut editor = blank_editor();
    let view = editor.editor_state.view.clone();
    start_selection_at(
        &mut editor,
        view.tick_to_x(2400.0),
        view.key_to_y(60) + view.zoom_y / 2.0,
    );

    // 向右拖到 tick=5000（落在单元 [3840, 5760) 内）→ 覆盖 [1920, 5760)
    move_to(&mut editor, 5000.0, 56);

    let (start_tick, current_tick, _, current_key, _, current_y) = selecting_state(&editor);
    assert_eq!(start_tick, 1920.0, "锚点端固定不动");
    assert_eq!(
        current_tick, 5760.0,
        "移动端应覆盖鼠标所在单元的高边：floor(5000/1920)*1920 + 1920 = 5760"
    );
    assert_eq!(current_key, 56);
    assert_eq!(
        current_y,
        view.key_to_y(56) + view.zoom_y,
        "移动中 current_y 应对齐 key 56 底线，而非像素 pos.y"
    );
}

#[test]
fn test_reverse_drag_keeps_direction_and_covers_touched_cells() {
    let mut editor = blank_editor();
    let view = editor.editor_state.view.clone();
    start_selection_at(
        &mut editor,
        view.tick_to_x(2400.0),
        view.key_to_y(60) + view.zoom_y / 2.0,
    );

    // 向左拖到 tick=1500（落在单元 [0, 1920) 内）
    move_to(&mut editor, 1500.0, 60);

    let (start_tick, current_tick, ..) = selecting_state(&editor);
    // 方向保持：start = 锚点端（单元高边 3840），current = 鼠标端（单元低边 0）
    assert_eq!(
        start_tick, 3840.0,
        "向左拖时锚点端应为其单元高边（选框右边界在格线上）"
    );
    assert_eq!(
        current_tick, 0.0,
        "向左拖时鼠标端应为其单元低边（选框左边界在格线上）"
    );
    // 覆盖的单元并集 = [0, 1920) ∪ [1920, 3840) = [0, 3840)
    assert_eq!(start_tick.min(current_tick), 0.0);
    assert_eq!(start_tick.max(current_tick), 3840.0);
}

#[test]
fn test_marquee_shrinks_back_when_dragging_right_again() {
    // 锚点固定的意义：向左拖出后再拖回右侧，选框必须**回缩**而不是保持最宽
    let mut editor = blank_editor();
    let view = editor.editor_state.view.clone();
    start_selection_at(
        &mut editor,
        view.tick_to_x(2400.0),
        view.key_to_y(60) + view.zoom_y / 2.0,
    );

    move_to(&mut editor, 1500.0, 60);
    let (s1, c1, ..) = selecting_state(&editor);
    assert_eq!((s1, c1), (3840.0, 0.0), "向左拖：覆盖 [0, 3840)");

    // 拖回 tick=3000（单元 [1920, 3840)）→ 应回缩到锚点单元
    move_to(&mut editor, 3000.0, 60);
    let (s2, c2, ..) = selecting_state(&editor);
    assert_eq!(
        (s2, c2),
        (1920.0, 3840.0),
        "拖回右侧后选框必须回缩到锚点单元（锚点未被鼠标端覆盖，不能单向扩张）"
    );
}

// ===== 精度跟随：四个档位 =====

/// 四个精度档位下，两端都必须是 `snap_precision` 的整数倍（落格线），
/// 且移动端恰好覆盖鼠标所在单元。
#[test]
fn test_precision_follows_snap_setting_four_levels() {
    // PPQ 1920 下的 1/4、1/8、1/16、1/32
    for precision in [1920.0_f32, 960.0, 480.0, 240.0] {
        let mut editor = blank_editor();
        editor.editor_state.view.snap_precision = precision;

        let view = editor.editor_state.view.clone();
        // 按下 tick=0（单元低边 0），向右拖到 tick=700
        start_selection_at(
            &mut editor,
            view.tick_to_x(0.0),
            view.key_to_y(60) + view.zoom_y / 2.0,
        );
        move_to(&mut editor, 700.0, 60);

        let (start_tick, current_tick, ..) = selecting_state(&editor);
        let expected_current = (700.0 / precision).floor() * precision + precision;
        assert_eq!(
            start_tick, 0.0,
            "精度 {precision}：起点应落在格线（锚点单元低边）"
        );
        assert_eq!(
            current_tick, expected_current,
            "精度 {precision}：移动端应覆盖鼠标所在单元高边"
        );
        assert_eq!(
            current_tick % precision,
            0.0,
            "精度 {precision}：框边界必须落在格线上（量化契约）"
        );
        assert!(
            current_tick > 700.0,
            "精度 {precision}：移动端必须覆盖鼠标所在单元（不小于鼠标位置）"
        );
    }
}

// ===== Spring 模式 / 橡皮擦 / Y 向工具：同口径 =====

#[test]
fn test_spring_mode_uses_same_quantization() {
    let mut editor = blank_editor();
    editor.editor_state.view.selection_box_mode = SelectionBoxMode::Spring;

    let view = editor.editor_state.view.clone();
    start_selection_at(
        &mut editor,
        view.tick_to_x(2400.0),
        view.key_to_y(60) + view.zoom_y / 2.0,
    );
    let (start_tick, current_tick, start_key, _, start_y, current_y) = selecting_state(&editor);
    // Spring 只影响渲染动画，不影响存储值：与 Direct 完全一致
    assert_eq!((start_tick, current_tick), (1920.0, 3840.0));
    assert_eq!((start_key, start_y), (60, view.key_to_y(60)));
    assert_eq!(current_y, view.key_to_y(60) + view.zoom_y);
}

#[test]
fn test_eraser_shift_selection_uses_same_quantization() {
    let mut editor = blank_editor();
    editor.editor_state.tool = Tool::Eraser; // 默认 EraserBehavior::Default

    let view = editor.editor_state.view.clone();
    let x = view.tick_to_x(2400.0);
    let y = view.key_to_y(60) + view.zoom_y / 2.0;
    // Shift + 空白处 → 框选删除
    editor.handle_eraser_pressed(Point::new(x, y), true, None);

    let (start_tick, current_tick, start_key, _, start_y, current_y) = selecting_state(&editor);
    assert_eq!(
        (start_tick, current_tick),
        (1920.0, 3840.0),
        "橡皮擦框选与指针框选同口径（单元覆盖式量化）"
    );
    assert_eq!(start_key, 60);
    assert_eq!(start_y, view.key_to_y(60), "橡皮擦框选起点 Y 对齐 key 线");
    assert_eq!(current_y, view.key_to_y(60) + view.zoom_y);
}

#[test]
fn test_y_select_tool_y_full_range_x_quantized() {
    let mut editor = blank_editor();
    editor.editor_state.tool = Tool::PointerYSelect;

    let view = editor.editor_state.view.clone();
    start_selection_at(
        &mut editor,
        view.tick_to_x(2400.0),
        view.key_to_y(60) + view.zoom_y / 2.0,
    );

    let (start_tick, current_tick, start_key, current_key, start_y, current_y) =
        selecting_state(&editor);
    // Y 维度自动覆盖全部可见键（0..=127）——行为不变
    assert_eq!(start_key, 127);
    assert_eq!(current_key, 0);
    assert_eq!(start_y, view.key_to_y(127));
    assert_eq!(current_y, view.key_to_y(0) + view.zoom_y);
    // X 维度同样走单元覆盖式量化
    assert_eq!((start_tick, current_tick), (1920.0, 3840.0));
}

// ===== 端到端：框缘贴边音符确定性地不入选 =====

#[test]
fn test_marquee_edge_note_on_grid_line_stays_out() {
    // 两条格线上的音符：N1@1920、N2@3840（1/4 精度）
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(1920.0, 60, 480.0), Note::new(3840.0, 60, 480.0)],
    );

    let view = editor.editor_state.view.clone();
    // 按下 tick=1000（单元 [0,1920)），向右拖到 tick=3700（仍在单元 [1920,3840)）
    start_selection_at(
        &mut editor,
        view.tick_to_x(1000.0),
        view.key_to_y(60) + view.zoom_y / 2.0,
    );
    move_to(&mut editor, 3700.0, 60);

    let (start_tick, current_tick, ..) = selecting_state(&editor);
    assert_eq!(
        (start_tick, current_tick),
        (0.0, 3840.0),
        "覆盖鼠标触碰过的单元 [0,1920) ∪ [1920,3840) = [0, 3840)"
    );

    // 框右边界恰好落在 N2 的起点 3840 上 → 半开区间下 N2 不入选
    assert_eq!(
        editor.selected_notes_count(),
        1,
        "框 [0,3840) 应只选中 N1@1920：起点恰等于框右边界的 N2 必须排除"
    );
    assert!(
        editor.editor_state.interaction.selected_notes.contains(&0),
        "入选的必须是 N1（索引 0）"
    );

    // 继续拖过格线到 tick=3900（进入单元 [3840,5760)）→ N2 才被覆盖
    move_to(&mut editor, 3900.0, 60);
    let (_, current_tick, ..) = selecting_state(&editor);
    assert_eq!(current_tick, 5760.0, "越过格线后移动端跳到下一单元高边");
    assert_eq!(
        editor.selected_notes_count(),
        2,
        "鼠标越入 N2 所在单元后，N2 入选（单元覆盖语义，非误选）"
    );
}
