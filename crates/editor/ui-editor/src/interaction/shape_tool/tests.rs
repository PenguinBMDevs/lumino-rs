//! 形状工具测试：描边（几何解析）/ 填充（格点 + 切分）/ 交互守卫

use super::confirm::{chop_cells, outline_notes};
use crate::Editor;
use crate::tests::test_helpers::seed_notes;
use lumino_editor_state::ShapeKind;
use std::collections::BTreeMap;

/// 构造一个非 Conductor 轨（track 1，空）、吸附精度 = 1 的编辑器
fn test_editor() -> Editor {
    let mut editor = Editor::new();
    // 初始化 document 与 track 1（形状工具在 track 0 不可用），初始无音符
    seed_notes(&mut editor, 2, 1, &[]);
    // 吸附精度 1 tick，使格点对齐整数 tick / key
    editor.editor_state.view.snap_precision = 1.0;
    editor
}

/// 音高行 → 该行音符区间（按起点升序），用于断言「描边逐音高行覆盖」而不是「格点数」
fn rows(editor: &Editor) -> Vec<(u16, Vec<(u32, u32)>)> {
    let mut by_key: BTreeMap<u16, Vec<(u32, u32)>> = BTreeMap::new();
    for n in editor.editor_state.data.current_track_notes().iter() {
        by_key
            .entry(n.key as u16)
            .or_default()
            .push((n.start_tick, n.end_tick));
    }
    for v in by_key.values_mut() {
        v.sort_unstable();
    }
    by_key.into_iter().collect()
}

#[test]
fn test_rectangle_outline_produces_notes() {
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    // 拖出 0..4 × key 60..64 的矩形轮廓
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(editor.editor_state.shape_tool.has_pending());
    // 拖拽过短校验：外接框应被规范化记录
    assert_eq!(
        editor.editor_state.shape_tool.shapes[0].rect,
        (0.0, 60.0, 4.0, 64.0)
    );
    let ok = editor.confirm_shape_tool();
    assert!(ok);
    // 描边走与曲线工具轮廓同源的蜘蛛网式逐音高行解析：
    // - 上下两条水平边（key 60 / 64）各得一条贯通整条边长的音符；
    // - 左右两条竖直边逐音高行各得 1 tick（竖直段不与平坦段共享长度，
    //   与 `paths::line_notes` 的「时间上不动的竖直段单独成片」口径一致）；
    // - 左竖直边在与水平边**同起点**的行（60 / 64）上被 `keep_longest` 合并掉，
    //   右竖直边终点的同名行音符（起点 4 ≠ 0）保留 → 角点各多一条 1 tick。
    assert_eq!(
        rows(&editor),
        vec![
            (60, vec![(0, 4), (4, 5)]),
            (61, vec![(0, 1), (4, 5)]),
            (62, vec![(0, 1), (4, 5)]),
            (63, vec![(0, 1), (4, 5)]),
            (64, vec![(0, 4), (4, 5)]),
        ],
        "矩形描边 = 四条边逐音高行无缝覆盖（不再是 5×5 - 3×3 = 16 个定长格点）"
    );
    assert_eq!(editor.editor_state.data.current_track_note_count(), 10);
    // 确认后待确认列表清空
    assert!(!editor.editor_state.shape_tool.has_pending());
}

/// 圆形没有 `shape_vertices`（渲染走椭圆参数曲线）：描边由椭圆采样折线解析得到，
/// 口径必须与矩形/三角形一致（同样不用吸附精度）。
#[test]
fn test_circle_outline_produces_per_row_notes() {
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Circle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool());

    let got = rows(&editor);
    // 椭圆中心行 key 62、半径 2 行：上半弧只走 62..64、下半弧只走 60..62；
    // 每个中间行在所属半弧上「上行进一次、下行再进一次」→ 各 2 条音符；
    // 最高/最低行只被进入一次 → 各 1 条。起点都是解析交点，不再是格点。
    assert_eq!(
        got.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        vec![60, 61, 62, 63, 64],
        "圆描边必须逐音高行覆盖 60..=64"
    );
    assert_eq!(got[0].1.len(), 1, "最低行只被下半弧进入一次");
    assert_eq!(got[4].1.len(), 1, "最高行只被上半弧进入一次");
    for (key, v) in got.iter().take(4).skip(1) {
        assert_eq!(v.len(), 2, "中间行 {key} 上下行各进入一次");
    }
    assert_eq!(editor.editor_state.data.current_track_note_count(), 8);
    // 不再是「每格一条 snap 长音符」：长度由解析交点决定，存在跨多 tick 的音符
    // （0..4 × 60..64 的圆描边恰好也是 8 条，故不能用条数当判据）
    assert!(
        editor
            .editor_state
            .data
            .current_track_notes()
            .iter()
            .any(|n| n.end_tick - n.start_tick > 1),
        "圆描边不得退化为「每格一条 snap 长音符」"
    );
}

#[test]
fn test_filled_rectangle_produces_interior_notes() {
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = true;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
    let ok = editor.confirm_shape_tool();
    assert!(ok);
    // 填充矩形 = 5×5 = 25 格
    assert_eq!(editor.editor_state.data.current_track_note_count(), 25);
}

#[test]
fn test_cancel_clears_pending() {
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Rectangle);
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(editor.editor_state.shape_tool.has_pending());
    editor.cancel_shape_tool();
    assert!(!editor.editor_state.shape_tool.has_pending());
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
}

#[test]
fn test_conductor_track_rejects_shape() {
    let mut editor = test_editor();
    editor.editor_state.data.current_track = 0;
    editor.set_shape(ShapeKind::Rectangle);
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
    // Conductor 轨道（track 0）整工具不可用，不应开始拖拽
    assert!(!editor.editor_state.shape_tool.has_pending());
}

#[test]
fn test_fill_bucket_marks_existing_shape() {
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Rectangle);
    // 先拉出轮廓（填充桶关闭）
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(!editor.editor_state.shape_tool.shapes[0].filled);
    // 开启填充桶并点选图形内部（命中）→ 标记填充
    editor.editor_state.shape_tool.fill_enabled = true;
    editor.handle_shape_tool_pressed(2.0, 62.0, false);
    assert!(editor.editor_state.shape_tool.shapes[0].filled);
}

/// Shift 拉出：尺寸直接吃鼠标原始坐标（绕过 key/音符精度吸附），再套正图形约束。
///
/// 用非整数原始坐标 (10.3, 63.7) 拖拽（宽高均不整除网格）。若错误地先吸附再约束
/// （(10, 64)），描边音符的几何会与基于原始坐标不同——据此证明确实走了原始坐标。
#[test]
fn test_shift_uses_raw_mouse_coords_for_square() {
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    // 模拟 host 通道：Shift 按下（released 路径读取 shift_pressed 决定约束）
    editor.shift_pressed = true;
    // 起点 (0.0, 60.0)，Shift 拖到非网格坐标 (10.3, 63.7)（原始浮点）
    editor.handle_shape_tool_pressed(0.0, 60.0, true);
    editor.handle_shape_tool_moved(10.3, 63.7);
    editor.handle_shape_tool_released();
    // 存储的外接框应为原始鼠标坐标（未吸附到网格）：若被吸附会变成 (0,60,10,64)
    assert_eq!(
        editor.editor_state.shape_tool.shapes[0].rect,
        (0.0, 60.0, 10.3, 63.7),
        "Shift 拖拽应直接吃鼠标原始坐标，而非先吸附到网格"
    );
    let ok = editor.confirm_shape_tool();
    assert!(ok);

    // 与 confirm 一致的屏幕像素尺度（tick/key 每单位像素数）
    let px_per_tick = editor.editor_state.view.zoom_x;
    let px_per_key = editor.editor_state.view.zoom_y;

    // 预期：基于原始鼠标坐标 (10.3, 63.7) 在「屏幕空间」套 Shift 约束
    // （正方形：min(宽_px, 高_px)）后走描边几何解析，逐音符一致。
    // 注意不能用 `shape_cells` 当预期——描边已不是「按 snap 枚举格点」。
    let mut expected: Vec<(i64, u16, i64)> = outline_notes(
        ShapeKind::Rectangle,
        (0.0, 60.0, 10.3, 63.7),
        true,
        px_per_tick,
        px_per_key,
        editor.editor_state.view.key_count as i32,
    )
    .into_iter()
    .map(|(t, k, l)| (t.round() as i64, k, (l * 1000.0).round() as i64))
    .collect();
    expected.sort_unstable();
    let mut actual: Vec<(i64, u16, i64)> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .map(|n| {
            (
                n.start_tick as i64,
                n.key as u16,
                ((n.end_tick - n.start_tick) as f32 * 1000.0).round() as i64,
            )
        })
        .collect();
    actual.sort_unstable();
    assert_eq!(
        actual, expected,
        "确认生成的描边音符应逐条等于「原始鼠标外接框 + 屏幕空间 Shift 约束」的几何结果"
    );
}

// ── 切分档位（x 分音符填充）同步 ──

#[test]
fn test_filled_shape_uses_division_step() {
    // 填充矩形 0..4 × key 60..64，snap = 1、ppq = 480：
    // 四分音符档（480 tick 太长 → 每行只有 1 条）；改用 1/8 档看切分效果。
    let mut editor = test_editor();
    editor.editor_state.view.ppq = 480;
    editor.set_shape(ShapeKind::Rectangle);
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
    editor.set_shape(ShapeKind::Rectangle);
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
    editor.set_shape(ShapeKind::Rectangle);
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

/// 退化拖拽（宽或高为 0，形状工具允许单轴拖动）不得 panic，且仍给出该行/该列的音符
#[test]
fn test_degenerate_outline_shapes_do_not_panic() {
    // 纯水平拖拽：高为 0（key 不变）→ 单行一条贯通音符
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(100.0, 60.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool(), "退化描边仍应生成音符");
    assert_eq!(rows(&editor), vec![(60, vec![(0, 100)])]);

    // 纯竖直拖拽：宽为 0 → 逐音高行各 1 tick
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Rectangle);
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
    editor.set_shape(ShapeKind::Rectangle);
    editor.set_ctrl_pressed(true);
    editor.handle_shape_tool_pressed(2.0, 62.0, false);
    assert!(
        editor.take_fill_division_dialog_request(),
        "形状工具 Ctrl+单击应置位弹窗请求"
    );
    assert!(
        !editor.editor_state.shape_tool.has_pending(),
        "Ctrl+单击不应开始拖拽"
    );
}
