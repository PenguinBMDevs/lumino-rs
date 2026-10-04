//! 三角形朝向回归测试（从 `tests.rs` 拆出，单文件长度纪律见 REF-001）
//!
//! **语义**：顶点始终朝**拖拽起点**那一侧的 key 边（[`ShapeSpec::apex_high`]）——
//! 向下拉 ⇒ 顶点在高音高侧 ⇒ 横卷帘屏幕上**正立**；向上拉 ⇒ **倒立**；
//! 拖拽过程中越过起点 ⇒ 实时翻转。key 轴正向 = 音高更高
//! （横卷帘 `ViewState::key_to_y`、纵卷帘 `Editor::tick_key_to_pos_f32`）。
//!
//! 覆盖四条腿：预览/渲染用的顶点（`shape_vertices`）、描边（几何解析逐音高行）、
//! 填充（`point_in_shape` → 格点）、屏幕空间朝向（与渲染同源的 `line_pos_screen_pos`）。

use crate::Editor;
use crate::tests::test_helpers::seed_notes;
use lumino_editor_state::shape_tool::{ShapeSpec, shape_vertices};
use lumino_editor_state::{DrawnShapeSource, ShapeKind};
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

/// 三角形某次拖拽的几何参数（起点 → 当前点，不松手）
fn drag_triangle(editor: &mut Editor, start: (f32, f32), current: (f32, f32)) -> ShapeSpec {
    editor.set_shape(ShapeKind::Triangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(start.0, start.1, false);
    editor.handle_shape_tool_moved(current.0, current.1);
    editor
        .editor_state
        .shape_tool
        .preview_rect(false)
        .expect("拖拽中应有实时预览")
        .0
}

/// 顶点 / 底边两端在**屏幕**上的坐标（与渲染同源的 `line_pos_screen_pos`）
///
/// 返回 `(顶点, 底边左端, 底边右端)`：横卷帘下底边两端屏幕 y 相同、顶点在其上方/下方；
/// 纵卷帘下底边两端屏幕 x 相同、顶点在其右侧/左侧。
fn screen_verts(editor: &Editor, spec: ShapeSpec) -> ((f32, f32), (f32, f32), (f32, f32)) {
    let verts = shape_vertices(
        spec,
        editor.editor_state.view.zoom_x,
        editor.editor_state.view.zoom_y,
    )
    .expect("三角形有顶点");
    let screen = |i: usize| {
        let p = editor.line_pos_screen_pos(verts[i]);
        (p.x, p.y)
    };
    (screen(2), screen(0), screen(1))
}

/// 向下拉（起点 key 大 → 当前 key 小）⇒ 顶点在高音高侧 ⇒ 横卷帘屏幕上**正立**
#[test]
fn test_drag_down_produces_upright_triangle() {
    let mut editor = test_editor();
    let spec = drag_triangle(&mut editor, (0.0, 64.0), (4.0, 60.0));
    assert!(spec.apex_high, "向下拉 ⇒ 顶点朝起点的高音高侧");
    let (apex, base_l, base_r) = screen_verts(&editor, spec);
    assert_eq!(base_l.1, base_r.1, "底边两端应在同一音高行");
    assert!(
        apex.1 < base_l.1,
        "横卷帘：顶点必须在屏幕上方（apex.y={} < base.y={}）",
        apex.1,
        base_l.1
    );

    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool(), "三角形描边应生成音符");
    // 底边（key 60）贯通 0..4 + 右斜边离开底边的 1 tick 角点；
    // 中间行两条斜边各穿过一次、随行高向顶点 (2,64) 收拢；顶点行收成 1 条。
    assert_eq!(
        rows(&editor),
        vec![
            (60, vec![(0, 4), (4, 5)]),
            (61, vec![(0, 1), (3, 4)]),
            (62, vec![(1, 2), (3, 4)]),
            (63, vec![(1, 2), (2, 3)]),
            (64, vec![(2, 3)]),
        ],
        "正立：宽行在下、顶点行在上"
    );
}

/// 向上拉（起点 key 小 → 当前 key 大）⇒ 顶点在低音高侧 ⇒ 横卷帘屏幕上**倒立**
#[test]
fn test_drag_up_produces_inverted_triangle() {
    let mut editor = test_editor();
    let spec = drag_triangle(&mut editor, (0.0, 60.0), (4.0, 64.0));
    assert!(!spec.apex_high, "向上拉 ⇒ 顶点朝起点的低音高侧");
    let (apex, base_l, base_r) = screen_verts(&editor, spec);
    assert_eq!(base_l.1, base_r.1, "底边两端应在同一音高行");
    assert!(
        apex.1 > base_l.1,
        "横卷帘：顶点必须在屏幕下方（apex.y={} > base.y={}）",
        apex.1,
        base_l.1
    );

    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool(), "三角形描边应生成音符");
    // 与「向下拉」逐行镜像：底边（key 64）贯通 0..4 + 角点 1 tick，
    // 中间行随行高向顶点 (2,60) 收拢，顶点行收成 1 条
    assert_eq!(
        rows(&editor),
        vec![
            (60, vec![(2, 3)]),
            (61, vec![(1, 2), (2, 3)]),
            (62, vec![(1, 2), (3, 4)]),
            (63, vec![(0, 1), (3, 4)]),
            (64, vec![(0, 4), (4, 5)]),
        ],
        "倒立：宽行在上、顶点行在下（上一条正立用例的逐行镜像）"
    );
    // 登记的几何必须带上朝向：外接框已规范化、朝向无法反推，
    // 丢了它鼠标工具的高亮/命中会把倒三角画成镜像
    match &editor.editor_state.shape_select.shapes()[0].source {
        DrawnShapeSource::Shape { apex_high, .. } => {
            assert!(!apex_high, "登记来源应保留倒立朝向");
        }
        other => panic!("期望 Shape 几何，实际 {other:?}"),
    }
}

/// 拖拽过程中越过起点 ⇒ 预览**实时**翻面（松手前已可见），确认结果与最终朝向一致
#[test]
fn test_reversing_drag_flips_triangle_live() {
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Triangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(0.0, 64.0, false);

    // ① 向下拉：正立
    editor.handle_shape_tool_moved(4.0, 60.0);
    let spec = editor
        .editor_state
        .shape_tool
        .preview_rect(false)
        .expect("拖拽中")
        .0;
    assert!(spec.apex_high);
    let (apex, base_l, _) = screen_verts(&editor, spec);
    assert!(apex.1 < base_l.1, "向下拉时预览为正立");

    // ② 拉过起点到上方：高度变号 ⇒ 预览立即翻为倒立（未松手）
    editor.handle_shape_tool_moved(4.0, 68.0);
    let spec = editor
        .editor_state
        .shape_tool
        .preview_rect(false)
        .expect("拖拽中")
        .0;
    assert!(!spec.apex_high, "越过起点 ⇒ 朝向实时翻转");
    let (apex, base_l, _) = screen_verts(&editor, spec);
    assert!(apex.1 > base_l.1, "越过起点后预览为倒立");

    // ③ 再拉回下方：翻回正立，松手确认的朝向与最后一次预览一致
    editor.handle_shape_tool_moved(4.0, 58.0);
    let spec = editor
        .editor_state
        .shape_tool
        .preview_rect(false)
        .expect("拖拽中")
        .0;
    assert!(spec.apex_high, "再拉回下方 ⇒ 翻回正立");
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool());
    // 最终朝向取松手时的方向（正立）：底边在 key 58（拖拽当前点）、顶点在 key 64（起点）
    assert_eq!(
        rows(&editor),
        vec![
            (58, vec![(0, 4), (4, 5)]),
            (59, vec![(0, 1), (4, 5)]),
            (60, vec![(1, 2), (3, 4)]),
            (61, vec![(1, 2), (3, 4)]),
            (62, vec![(1, 2), (3, 4)]),
            (63, vec![(2, 3)]),
            (64, vec![(2, 3)]),
        ],
        "松手时为正立（底边 key 58、顶点 key 64）"
    );
}

/// 纵向卷帘转置：key 轴映射到屏幕 X，顶点仍朝拖拽起点侧（向左拉 ⇒ 顶点朝右）
#[test]
fn test_triangle_apex_follows_drag_direction_on_vertical_roll() {
    let mut editor = test_editor();
    editor.editor_state.is_vertical_roll = true;
    // 起点 key 64 → 当前 key 60（屏幕上向左拖）⇒ 顶点在 key 大的一侧 ⇒ 屏幕朝右
    let spec = drag_triangle(&mut editor, (0.0, 64.0), (4.0, 60.0));
    let (apex, base_l, base_r) = screen_verts(&editor, spec);
    assert_eq!(base_l.0, base_r.0, "纵卷帘：底边两端应在同一屏幕 X");
    assert!(
        apex.0 > base_l.0,
        "纵卷帘：顶点应指向 key 大的一侧（apex.x={} > base.x={}）",
        apex.0,
        base_l.0
    );

    // 反向拖（向右）⇒ 顶点在 key 小的一侧 ⇒ 屏幕朝左
    let mut editor = test_editor();
    editor.editor_state.is_vertical_roll = true;
    let spec = drag_triangle(&mut editor, (0.0, 60.0), (4.0, 64.0));
    let (apex, base_l, base_r) = screen_verts(&editor, spec);
    assert_eq!(base_l.0, base_r.0, "纵卷帘：底边两端应在同一屏幕 X");
    assert!(
        apex.0 < base_l.0,
        "纵卷帘：反向拖拽时顶点指向 key 小的一侧（apex.x={} < base.x={}）",
        apex.0,
        base_l.0
    );
}

/// 填充腿同样吃朝向：向下拉 ⇒ 宽行在低音高侧；向上拉 ⇒ 宽行在高音高侧
#[test]
fn test_filled_triangle_follows_drag_direction() {
    // 向下拉（正立）：底边行 key 60 最宽 5 格，顶点行 64 最窄 1 格
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Triangle);
    editor.editor_state.shape_tool.fill_enabled = true;
    editor.handle_shape_tool_pressed(0.0, 64.0, false);
    editor.handle_shape_tool_moved(4.0, 60.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool(), "填充三角形应生成音符");
    let widths: Vec<usize> = rows(&editor).iter().map(|(_, v)| v.len()).collect();
    assert_eq!(widths, vec![5, 3, 3, 1, 1], "正立：宽行在 key 60");

    // 向上拉（倒立）：同一外接框，宽行翻到 key 64
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Triangle);
    editor.editor_state.shape_tool.fill_enabled = true;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool(), "填充三角形应生成音符");
    let widths: Vec<usize> = rows(&editor).iter().map(|(_, v)| v.len()).collect();
    assert_eq!(widths, vec![1, 1, 3, 3, 5], "倒立：宽行在 key 64");
}
