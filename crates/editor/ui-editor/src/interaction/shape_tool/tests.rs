//! 形状工具测试：描边（几何解析）/ 填充（格点 + 切分）/ 交互守卫
//!
//! 三角形朝向回归见 `tests_triangle.rs`（单文件长度纪律，REF-001）。

use super::confirm::{chop_cells, outline_notes};
use crate::Editor;
use crate::tests::test_helpers::seed_notes;
use lumino_editor_state::ShapeKind;
use lumino_editor_state::shape_tool::ShapeSpec;
use std::collections::BTreeMap;

/// 构造一个非 Conductor 轨（track 1，空）、吸附精度 = 1 的编辑器
fn test_editor() -> Editor {
    let mut editor = Editor::new();
    // 初始化 document 与 track 1（形状工具在 track 0 不可用），初始无音符
    seed_notes(&mut editor, 2, 1, &[]);
    // 吸附精度 1 tick，使格点对齐整数 tick / key
    editor.editor_state.view.snap_precision = 1.0;
    // 关闭最小长度下限：本文件多数用例钉的是**逐音高行覆盖几何**（4 tick 的合成
    // 图形里竖直段只有 1 tick），下限会把整条图形补齐、几何断言失去意义。
    // 下限本身由 `test_outline_min_length_*` 单独钉死（默认 128 分音符）。
    editor.editor_state.line_tool.set_min_note_division(None);
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

/// 全局绘制粗细（纵向 key 层数）作用在**形状描边**上：同一个矩形，粗细 3 层后
/// 音符带向上加厚 2 行（key 60..=66），且每一行都被覆盖。
///
/// 断言的是**带状覆盖的几何不变量**而不是某一个定值条数：多层展开后不同来源
/// （水平边 / 竖直边 / 相邻行的层副本）会在同一 `(tick, key)` 上相遇，由既有的
/// 「同 tick 同 key 只留最长」规则收敛——最终条数取决于路径行走顺序，钉死具体
/// 条数会把测试绑死在实现细节上。逐层精确性由 `stroke_band` 的单测钉死。
#[test]
fn test_rectangle_outline_expands_by_global_thickness() {
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.brush.set_thickness(3);
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool());

    let got = rows(&editor);
    let keys: Vec<u16> = got.iter().map(|(k, _)| *k).collect();
    assert_eq!(
        keys,
        vec![60, 61, 62, 63, 64, 65, 66],
        "粗细 3 层应把 60..=64 的描边带向上加厚到 66（每行都有音符，无空洞）"
    );
    assert!(
        got.iter().all(|(_, spans)| !spans.is_empty()),
        "加厚后的每一行都必须被音符覆盖"
    );

    let total = editor.editor_state.data.current_track_note_count();
    let base = 10; // 粗细 1 时该矩形的描边条数（见上一个用例）
    assert!(
        (base..=base * 3).contains(&total),
        "条数应落在 [单层条数, 单层条数 × 3] 内（层副本会被同 tick 同 key 去重收缩），实际 {total}"
    );
}

/// 全局粗细对**竖直边**同样生效：矩形的左右竖边必须按厚度**横向**加宽，
/// 而不是永远停在 1 tick（或仅被最小长度下限撑到几十 tick）。
///
/// BUG 背景（用户报）：早先的加粗实现是"把中心线音符按 key 向上平移 N 层"——
/// 这对水平边成立（向上 = 垂直于线），但对竖边等于把线**拉长**：横向宽度原地不动，
/// 于是"改了粗细度，矩形竖边还是很细一条，宽度不匹配"。加粗必须沿**线段法线**做。
///
/// 取 `key 63` 作为量尺：下水平边的带铺到 62、上水平边的带从 64 起，只有左右两条
/// 竖边覆盖 63——两条竖边的宽度在这里能被**完整**量出来（在 60..62 行它们会和水平
/// 边合并成一条区间，量不出竖边自身的宽度）。
#[test]
fn test_rectangle_vertical_edges_widen_by_global_thickness() {
    // 真实尺度（默认 zoom：1 tick = 0.1px、1 key = 20px）：800 tick 宽已是 80px，
    // 不会被"像素尺度下几乎竖直"这类退化情形掩盖问题
    let draw = |thickness: u8| {
        let mut editor = test_editor();
        editor.set_shape(ShapeKind::Rectangle);
        editor.editor_state.shape_tool.fill_enabled = false;
        editor.brush.set_thickness(thickness);
        editor.handle_shape_tool_pressed(0.0, 60.0, false);
        editor.handle_shape_tool_moved(800.0, 64.0);
        editor.handle_shape_tool_released();
        assert!(editor.confirm_shape_tool());
        editor
    };
    let spans_at = |editor: &Editor, key: u16| -> Vec<(u32, u32)> {
        rows(editor)
            .into_iter()
            .find(|(k, _)| *k == key)
            .map(|(_, spans)| spans)
            .unwrap_or_default()
    };

    let view = test_editor();
    let zx = view.editor_state.view.zoom_x;
    let zy = view.editor_state.view.zoom_y;
    let extra = ((3 - 1) as f32 * zy / zx).round() as u32;
    assert!(
        extra > 1,
        "前置：该缩放下竖边应明显加宽（2 个 key 高 = {extra} tick）"
    );

    assert_eq!(
        spans_at(&draw(1), 63),
        vec![(0, 1), (800, 801)],
        "粗细 1（基准）：左右竖边各 1 tick"
    );
    assert_eq!(
        spans_at(&draw(3), 63),
        vec![(0, extra), (800, 800 + extra)],
        "粗细 3：左右竖边必须各按法线横向加宽到 {extra} tick"
    );
}

/// 覆盖探针：`(tick, key)` 这一格是否被某条音符盖住（半开区间 `[start, end)`，
/// 与音符的时间语义一致）。
///
/// 为什么要探针而不是「该行有没有音符」：`test_rectangle_outline_expands_by_global_thickness`
/// 只断言了 `60..=66` 每行非空——**行非空 ≠ 该行该 tick 有音符**。上一轮的水平边带覆盖了
/// 64/65/66 行的左半段，行非空成立，而右上角外角方格（tick > 800）空着却没人发现。
/// 几何断言必须落到「格」上，这是本次 BUG 逃逸的直接原因。
fn covers(editor: &Editor, tick: u32, key: u16) -> bool {
    editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .any(|n| n.key as u16 == key && n.start_tick <= tick && tick < n.end_tick)
}

/// 回归 BUG（用户报）：矩形**右上角外角在加粗后出现缺口**——音符没覆盖到角上。
///
/// 根因：加粗带由「每段折线一条单侧矩形」求并而成，两段各自的矩形在拐角**外侧**留下的
/// 那块方格没有被任何一段覆盖（矩形段止于 `tick = 800` / `key = 64`，谁都不越界到
/// `tick > 800 ∧ key > 64`）。预览是 `Stroke` + 默认 **Miter** 拼接（`shape_tool_box.rs`
/// 的 `frame.stroke`），拐角是实心的 → 生成音符必须补上同一块外角方格才叫「所见即所得」。
///
/// 量尺：粗细 3 层、默认缩放（1 tick = 0.1px、1 key = 20px）⇒ 偏移量 = 2 key 高
/// = 400 tick / 2 行；右上外角方格 = tick `800..1200` × key `64..66`。
#[test]
fn test_rectangle_corner_gap_is_filled_by_band() {
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.brush.set_thickness(3);
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(800.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool());

    // ① 右上角外角方格必须被覆盖（BUG 位置）
    assert!(
        covers(&editor, 1000, 65),
        "右上角外角方格（tick 1000 / key 65）必须被音符覆盖——预览是 Miter 实心角，\
         生成侧不能留缺口。实际该行区间：{:?}",
        rows(&editor)
            .into_iter()
            .filter(|(k, _)| *k == 65)
            .collect::<Vec<_>>()
    );
    assert!(
        covers(&editor, 801, 64),
        "外角方格紧贴角点的内边界也必须覆盖"
    );
    assert!(
        covers(&editor, 1199, 65),
        "外角方格最外一格（tick 1199 / key 65）"
    );
    // ② 其余三条边（下、左、右）的加宽带不受影响
    assert!(covers(&editor, 1000, 61), "右竖边加宽带（key 61）");
    assert!(covers(&editor, 200, 65), "上水平边加宽带（key 65）");
    assert!(covers(&editor, 200, 61), "下水平边加宽带（key 61）");
    // ③ 图形内部（偏移后的洞）必须保持空：不得为了补角把整块填满。
    //    单侧偏向（向上优先、其次向右）⇒ 洞 = 原矩形平移 (+400 tick, +2 key) 后的内边界：
    //    只有 key 63 行露出洞，且洞恰为 tick 400..800。
    assert!(!covers(&editor, 600, 63), "矩形内部的洞不应有音符");
    assert!(
        covers(&editor, 399, 63),
        "左侧加宽带右边界（tick 399）在音符内"
    );
    assert!(
        !covers(&editor, 400, 63),
        "洞从左侧加宽带右边界 tick 400 起"
    );
    assert!(
        !covers(&editor, 799, 63),
        "洞到右侧加宽带左边界 tick 800 止（不含）"
    );
    assert!(
        covers(&editor, 800, 63),
        "右侧加宽带左边界 tick 800 在音符内"
    );
    assert!(
        covers(&editor, 600, 62),
        "底边加宽带向上铺到 62 行 → 该行整条被覆盖"
    );
}

/// **描边预览 == 实际写入**（「所见即生成」的结构不变式）。
///
/// 画布预览现在直接画 [`Editor::shape_outline_blocks`] 给出的音符块（此前是
/// `Stroke::width = 粗细 × 单 key 高` 的**居中**描边，产物却是**单侧向外**铺
/// `N-1` 层 —— 竖直边在粗细 2 时预览比产物宽一倍，用户报的"宽度显示 X2"）。
/// 这条把"两边同一份几何"钉死：任何一边的参数漂移（粗细 / zoom / 长度下限 /
/// key 过滤）都会让它红。
#[test]
fn test_outline_preview_blocks_match_generated_notes() {
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.brush.set_thickness(3);
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(800.0, 64.0);
    editor.handle_shape_tool_released();

    // 预览块 = 画布上真正画出来的东西
    let spec = editor.editor_state.shape_tool.shapes[0].spec();
    let mut preview: Vec<(i64, i64, u16)> = editor
        .shape_outline_blocks(spec, false)
        .into_iter()
        .map(|(a, b, k)| (a as i64, b as i64, k))
        .collect();
    preview.sort_unstable();
    assert!(!preview.is_empty(), "描边预览不得为空");

    // ① 预览必须是**向外拓展**的带（含右上角外角方格），不是居中描边：
    //    居中（±1.5 个 key）会下探到 key 59；向外铺 2 层则只到 60..66
    assert!(
        preview.iter().all(|(_, _, k)| (60..=66).contains(k)),
        "预览块只应落在 key 60..=66（向外铺），实际 {:?}",
        preview.iter().map(|(_, _, k)| *k).collect::<Vec<_>>()
    );
    let covers = |blocks: &[(i64, i64, u16)], tick: i64, key: u16| {
        blocks
            .iter()
            .any(|&(a, b, k)| k == key && a <= tick && tick < b)
    };
    assert!(covers(&preview, 1000, 65), "预览必须含右上角外角方格");

    assert!(editor.confirm_shape_tool());
    let mut written: Vec<(i64, i64, u16)> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .map(|n| (n.start_tick as i64, n.end_tick as i64, n.key as u16))
        .collect();
    written.sort_unstable();
    assert_eq!(
        preview, written,
        "预览块必须与实际写入的音符逐条一致（预览一套、产物一套 = 该 BUG 的根因）"
    );
}

/// 预览块必须用**补齐过长度**的音符（最小长度下限，默认 128 分音符）。
///
/// 竖直边逐音高行原本只有 1 tick（实机 < 1px）：若预览拿"未补齐"的几何去画，看到的
/// 是 1px 细线、√ 之后却是有长度的音符——又一处"预览一套、产物一套"。
#[test]
fn test_outline_preview_blocks_include_min_length_padding() {
    let mut editor = test_editor();
    // 恢复默认下限档位（`test_editor` 为几何断言关掉了它）
    editor.set_min_note_division(Some(
        lumino_editor_state::LineToolState::DEFAULT_MIN_NOTE_DIVISION,
    ));
    editor.set_shape(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.brush.set_thickness(3);
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(800.0, 64.0);
    editor.handle_shape_tool_released();

    let spec = editor.editor_state.shape_tool.shapes[0].spec();
    let mut preview: Vec<(i64, i64, u16)> = editor
        .shape_outline_blocks(spec, false)
        .into_iter()
        .map(|(a, b, k)| (a as i64, b as i64, k))
        .collect();
    preview.sort_unstable();
    let min_ticks = (editor.editor_state.view.ppq / 32).max(1) as i64;
    assert!(
        preview.iter().any(|(a, b, _)| b - a == min_ticks),
        "预览里应出现被补齐到 {min_ticks} tick 的音符（竖直边），实际 {preview:?}"
    );

    assert!(editor.confirm_shape_tool());
    let mut written: Vec<(i64, i64, u16)> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .map(|n| (n.start_tick as i64, n.end_tick as i64, n.key as u16))
        .collect();
    written.sort_unstable();
    assert_eq!(preview, written, "补齐长度也必须预览与写入同源");
}

/// 填充图形的预览不得画"随粗细变宽的描边"：填充腿根本不生成描边、也不吃粗细，
/// 画出来等于显示一个不会存在的产物。
#[test]
fn test_filled_shape_has_no_outline_preview_blocks() {
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = true;
    editor.brush.set_thickness(3);
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(800.0, 64.0);
    editor.handle_shape_tool_released();
    let spec = editor.editor_state.shape_tool.shapes[0].spec();
    assert!(
        editor.shape_outline_blocks(spec, true).is_empty(),
        "填充图形不生成描边 → 描边预览块必须为空"
    );
    assert!(
        !editor.shape_outline_blocks(spec, false).is_empty(),
        "同一图形按描边处理时必须有预览块（对照组：证明上面不是恒空）"
    );
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
        ShapeSpec {
            kind: ShapeKind::Rectangle,
            rect: (0.0, 60.0, 10.3, 63.7),
            shift_constrained: true,
            // 向上拉（起点 key 60 → 当前 63.7）；矩形忽略该字段，仅为几何参数完整
            apex_high: false,
        },
        px_per_tick,
        px_per_key,
        editor.editor_state.view.key_count as i32,
        crate::interaction::line_tool::paths::min_note_length_ticks(
            editor.min_note_division(),
            editor.editor_state.view.ppq,
        ),
        // 全局粗细：本用例的默认值（1 层），与编辑器实际生成口径同源
        editor.draw_thickness(),
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

    editor.set_shape(ShapeKind::Rectangle);
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
    editor.set_shape(ShapeKind::Rectangle);
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
    editor.set_shape(ShapeKind::Rectangle);
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
