//! 形状工具的填充分割 / 最小长度 / 退化输入测试。
//!
//! 由 `tests.rs` 拆出，保持单测试文件 ≤800 行（AGENTS.md 尺寸纪律）。

use super::*;

use super::super::confirm::chop_cells;
use crate::Editor;
use lumino_editor_state::ShapeKind;

#[test]
fn test_filled_shape_uses_division_step() {
    // 填充矩形 0..4 × key 60..64，snap = 1、ppq = 480：
    // 四分音符档（480 tick 太长 → 每行只有 1 条）；改用 1/8 档看切分效果。
    let mut editor = test_editor();
    editor.editor_state.view.ppq = 480;
    editor
        .editor_state
        .shape_tool
        .set_shape_kind(ShapeKind::Rectangle);
    editor.set_fill_division(Some(8)); // 八分音符 = 240 tick
    editor.editor_state.shape_tool.fill_enabled = true;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(480.0, 60.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool());

    let notes = editor.editor_state.data.current_track_notes();
    // 单行（key 60）：格点 0,1,2,...,480（snap=1）合并成 [0, 481) → 按 240 切
    let row: Vec<(u32, u32)> = notes
        .iter()
        .filter(|n| n.key == 60)
        .map(|n| (n.start_tick, n.end_tick))
        .collect();
    assert_eq!(
        row,
        vec![(0, 240), (240, 480), (480, 481)],
        "八分音符切分 + 尾部残段: {row:?}"
    );
}

/// 画 0..480 × 60..64 的矩形**描边**（空桶）并 √，返回编辑器（切分档位对比用）
fn confirm_wide_rect_outline(division: Option<u32>) -> Editor {
    let mut editor = test_editor();
    editor.editor_state.view.ppq = 480;
    editor
        .editor_state
        .shape_tool
        .set_shape_kind(ShapeKind::Rectangle);
    editor.set_fill_division(division);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(480.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool());
    editor
}

#[test]
fn test_outline_shape_ignores_division() {
    // 描边不在填充桶职责内：切分档位开启与否，生成的音符必须逐条一致
    // （与曲线工具同构——`fill_division` 只作用于填充区间，轮廓走 `path_notes`）。
    let without = confirm_wide_rect_outline(None);
    let without_rows = rows(&without);
    let without_count = without.editor_state.data.current_track_note_count();
    assert!(without_count > 0);

    let with = confirm_wide_rect_outline(Some(8));
    assert_eq!(
        rows(&with),
        without_rows,
        "描边逐音高行覆盖不受切分档位影响"
    );
    assert_eq!(
        with.editor_state.data.current_track_note_count(),
        without_count,
        "描边音符数不受切分档位影响"
    );
    // 描边长度由几何解析决定（水平边 = 整条边长 480 tick，竖直段 = 1 tick，角点另加
    // 1 tick），既不是「每格一条 snap 长音符」，也不是切分档的 x 分音符长度。
    assert_eq!(
        without_rows[0],
        (60, vec![(0, 480), (480, 481)]),
        "下边框 = 一条 480 tick 长音符 + 右角点 1 tick"
    );
    assert!(
        without_rows
            .iter()
            .flat_map(|(_, v)| v.iter())
            .any(|(a, b)| b - a == 1),
        "竖直边逐音高行 1 tick（竖直段不与平坦段共享长度）"
    );
}

#[test]
fn test_no_division_keeps_filled_shape_behaviour() {
    // 填充腿不受本次描边同步影响：未开切分 → 仍是每格一条 snap 长音符
    let mut editor = test_editor();
    editor
        .editor_state
        .shape_tool
        .set_shape_kind(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = true;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool());
    assert_eq!(editor.editor_state.data.current_track_note_count(), 25);
    assert!(
        editor
            .editor_state
            .data
            .current_track_notes()
            .iter()
            .all(|n| n.end_tick - n.start_tick == 1),
        "填充格音符长度仍为 snap"
    );
}

/// 回归 BUG：矩形/三角形的**竖直边**逐音高行只有 1 tick —— 1920 ppq 下是
/// 1/7680 个全音符，实机缩放下不足 1px（用户看到的是「音符宽度过小 / 像没生成」）。
/// 修复后：描边音符长度补齐到至少 128 分音符（= `ppq / 32`），水平边不受影响。
#[test]
fn test_outline_min_length_fills_short_vertical_segments() {
    let mut editor = test_editor();
    // `test_editor` 为几何断言关掉了下限，这里恢复默认档位（128 分音符）
    let division = lumino_editor_state::LineToolState::DEFAULT_MIN_NOTE_DIVISION;
    assert_eq!(division, 128, "默认最小长度档位 = 128 分音符");
    editor.set_min_note_division(Some(division));
    let min_ticks = (editor.editor_state.view.ppq / 32).max(1) as u32;

    editor
        .editor_state
        .shape_tool
        .set_shape_kind(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    // 0..3840 × key 60..64：水平边 3840 tick（远超下限，不被补齐）
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(3840.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool());

    let end = 3840 + min_ticks;
    assert_eq!(
        rows(&editor),
        vec![
            (60, vec![(0, 3840), (3840, end)]),
            (61, vec![(0, min_ticks), (3840, end)]),
            (62, vec![(0, min_ticks), (3840, end)]),
            (63, vec![(0, min_ticks), (3840, end)]),
            (64, vec![(0, 3840), (3840, end)]),
        ],
        "竖直边逐行音符补齐到 128 分音符，水平边保持 3840 tick"
    );
}

/// 修改接口：最小长度按 x 分音符档位换算（`4·ppq/x`）；`None` = 不设下限。
#[test]
fn test_outline_min_length_is_configurable() {
    let mut editor = test_editor();
    editor
        .editor_state
        .shape_tool
        .set_shape_kind(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(3840.0, 64.0);
    editor.handle_shape_tool_released();

    // 六十四分音符档（= ppq / 16）比默认的 128 分音符档长一倍
    editor.set_min_note_division(Some(64));
    let expected = (editor.editor_state.view.ppq as f32 / 16.0).round() as u32;
    assert!(editor.confirm_shape_tool());
    assert_eq!(
        rows(&editor)[1].1[0],
        (0, expected),
        "64 分音符档 = 4·ppq/64 = ppq/16"
    );

    // None = 关闭下限：竖直段回到几何原样 1 tick
    let mut editor = test_editor();
    editor.set_min_note_division(None);
    editor
        .editor_state
        .shape_tool
        .set_shape_kind(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(3840.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool());
    assert_eq!(
        rows(&editor)[1].1[0],
        (0, 1),
        "关闭下限后竖直段保持 1 tick（几何原样）"
    );
}

/// 退化拖拽（宽或高为 0，形状工具允许单轴拖动）不得 panic，且仍给出该行/该列的音符
#[test]
fn test_degenerate_outline_shapes_do_not_panic() {
    // 纯水平拖拽：高为 0（key 不变）→ 单行一条贯通音符
    let mut editor = test_editor();
    editor
        .editor_state
        .shape_tool
        .set_shape_kind(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(100.0, 60.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool(), "退化描边仍应生成音符");
    assert_eq!(rows(&editor), vec![(60, vec![(0, 100)])]);

    // 纯竖直拖拽：宽为 0 → 逐音高行各 1 tick
    let mut editor = test_editor();
    editor
        .editor_state
        .shape_tool
        .set_shape_kind(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(0.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool(), "退化描边仍应生成音符");
    assert_eq!(
        rows(&editor)
            .iter()
            .map(|(k, v)| (*k, v.len()))
            .collect::<Vec<_>>(),
        vec![(60, 1), (61, 1), (62, 1), (63, 1), (64, 1)]
    );
}

#[test]
fn test_chop_cells_merges_contiguous_and_keeps_gaps() {
    // 同 key：0,1,2 连续（snap=1）→ 一段 [0,3)；5 孤立 → 另一段 [5,6)
    let cells = vec![(0.0f32, 60u16), (1.0, 60), (2.0, 60), (5.0, 60)];
    let out = chop_cells(&cells, 1.0, 2.0);
    assert_eq!(
        out,
        vec![(0.0, 60, 2.0), (2.0, 60, 1.0), (5.0, 60, 1.0)],
        "合并连续格点后按步长切分，空隙独立成段"
    );
}

#[test]
fn test_shape_ctrl_click_requests_dialog() {
    // 形状工具下的 Ctrl+单击同样请求弹窗（与曲线工具填充桶一致）
    let mut editor = test_editor();
    editor
        .editor_state
        .shape_tool
        .set_shape_kind(ShapeKind::Rectangle);
    editor.set_ctrl_pressed(true);
    editor.handle_shape_tool_pressed(2.0, 62.0, false);
    assert!(
        editor
            .editor_state
            .line_tool
            .take_fill_division_dialog_request(),
        "形状工具 Ctrl+单击应置位弹窗请求"
    );
    assert!(
        !editor.editor_state.shape_tool.has_pending(),
        "Ctrl+单击不应开始拖拽"
    );
}
