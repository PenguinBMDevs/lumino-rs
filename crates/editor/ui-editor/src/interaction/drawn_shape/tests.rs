use super::hit::point_segment_distance;
use super::*;
use crate::tests::test_helpers::seed_notes;
use lumino_editor_state::{BezierAnchor, BrushStroke, ShapeKind, ShapeToolInteraction};
use lumino_message::Tool;

/// 0..4 × 60..64（snap = 1）的矩形**描边**音符数 = 10：
/// 上下水平边各 1 条贯通 4 tick、右竖直边逐音高行 5 条 1 tick、
/// 左竖直边 5 条中与水平边同起点的 2 条被 `keep_longest` 合并 → 存 3 条。
/// （旧的「按 snap 枚举轮廓格点」口径在同一矩形上是 16 条定长音符。）
const RECT_OUTLINE_NOTES: usize = 10;

/// 构造非 Conductor 轨（track 1）编辑器，吸附精度 = 1
fn test_editor() -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 2, 1, &[]);
    editor.editor_state.view.snap_precision = 1.0;
    editor
}

/// 为 DrawnShape 自身的行为测试登记一个“已确认图形”。
///
/// Bake 后真实确认链路不再登记矢量对象；这些用例只测试选择/拖动/删除/历史同步，
/// 因此直接构造等价的注册表条目，避免把“Bake 不登记”与“注册表行为”混在一起。
fn register_test_drawn_shape(editor: &mut Editor, source: DrawnShapeSource) {
    let track = editor.editor_state.data.current_track;
    let notes: Vec<ShapeNote> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .map(|n| ShapeNote {
            track,
            tick: n.start_tick as f32,
            key: n.key as u16,
            length: (n.end_tick - n.start_tick) as f32,
            velocity: n.velocity,
            channel: n.channel,
        })
        .collect();
    let group = editor.last_create_group();
    editor
        .editor_state
        .shape_select
        .add(track, group, source, notes);
}

/// 拉出并确认一个 0..4 × 60..64 的矩形描边（snap = 1），返回编辑器
///
/// 描边音符数见 [`RECT_OUTLINE_NOTES`]（不再是「每格一条」的 16 条）。
fn draw_and_confirm_rect() -> Editor {
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool(), "矩形应生成音符");
    register_test_drawn_shape(
        &mut editor,
        DrawnShapeSource::Shape {
            kind: ShapeKind::Rectangle,
            rect: (0.0, 60.0, 4.0, 64.0),
            shift_constrained: false,
            filled: false,
            apex_high: false,
        },
    );
    editor
}

fn note_keys(editor: &Editor) -> Vec<u16> {
    editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .map(|n| n.key as u16)
        .collect()
}

fn note_ticks(editor: &Editor) -> Vec<u32> {
    editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .map(|n| n.start_tick)
        .collect()
}

// ── 登记 ─────────────────────────────────────────────

#[test]
fn test_confirm_shape_bakes_notes_without_shape() {
    let mut editor = test_editor();
    draw_pending_rect(&mut editor);
    assert!(editor.confirm_shape_tool(), "矩形应生成音符");
    assert_eq!(
        editor.editor_state.shape_select.len(),
        0,
        "Bake 后不应登记可交互图形"
    );
    let entry = editor
        .editor_state
        .shape_store
        .iter()
        .next()
        .expect("DRAW-041B 应保留只读 ShapeModel");
    assert!(entry.is_visible());
    assert_eq!(entry.notes.len(), RECT_OUTLINE_NOTES);
    assert_eq!(
        entry.model.style.fill,
        lumino_core::shape::ShapeFill::Empty,
        "未填充形状的保留模型应沿用 Empty；032B 首次着色时再切 Fill"
    );
    assert!(
        entry.model.geometry.closed,
        "形状工具保留模型必须是闭合图形，否则区域着色只会退回描边"
    );
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        RECT_OUTLINE_NOTES,
        "矩形描边 = 上下边各 1 条 + 右竖直边 5 条 + 左竖直边 3 条"
    );

    assert!(editor.undo(), "Bake 应可撤销");
    assert!(
        editor.editor_state.shape_tool.has_pending(),
        "undo 应恢复待确认形状"
    );
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
    assert!(
        editor.editor_state.data.main_track_struct_dirty,
        "undo Bake 必须强制主轨段重建，否则 GPU 会残留 Bake 音符"
    );

    assert!(editor.redo(), "Bake 应可重做");
    assert!(
        !editor.editor_state.shape_tool.has_pending(),
        "redo 应再次丢弃形状"
    );
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        RECT_OUTLINE_NOTES
    );
}

#[test]
fn test_two_bakes_are_separate_history_groups() {
    let mut editor = test_editor();
    editor.set_tool(Tool::Curve);
    let first = vec![vec![
        BezierAnchor::new((0.0, 60.0)),
        BezierAnchor::new((240.0, 64.0)),
    ]];
    editor.editor_state.line_tool.paths = first;
    editor.editor_state.line_tool.recompute_auto_handles();
    assert!(editor.confirm_line_tool());
    let first_notes = editor.editor_state.data.current_track_note_count();
    assert!(first_notes > 0);

    // 第二次确认发生在合并窗口内，仍必须形成独立 group
    editor.editor_state.line_tool.paths = vec![vec![
        BezierAnchor::new((480.0, 60.0)),
        BezierAnchor::new((720.0, 64.0)),
    ]];
    editor.editor_state.line_tool.recompute_auto_handles();
    assert!(editor.confirm_line_tool());
    let both_notes = editor.editor_state.data.current_track_note_count();
    assert!(both_notes > first_notes);

    assert!(editor.undo(), "应只撤销第二次 Bake");
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        first_notes,
        "一次 undo 不得删掉两次 Bake 的全部音符"
    );
    assert_eq!(
        editor.editor_state.line_tool.paths.len(),
        1,
        "只应恢复第二次 Bake 的待确认曲线"
    );
    assert_eq!(
        editor.editor_state.shape_select.len(),
        0,
        "Bake 后始终没有可交互曲线对象"
    );
}

#[test]
fn test_confirm_curve_bakes_notes_without_shape() {
    let mut editor = test_editor();
    editor.editor_state.line_tool.paths = vec![vec![
        BezierAnchor::new((0.0, 60.0)),
        BezierAnchor::new((240.0, 64.0)),
    ]];
    editor.editor_state.line_tool.recompute_auto_handles();
    assert!(editor.confirm_line_tool(), "曲线应生成音符");
    assert_eq!(
        editor.editor_state.shape_select.len(),
        0,
        "Bake 后不应保留曲线对象"
    );
    assert!(
        !editor.editor_state.data.current_track_notes().is_empty(),
        "Bake 后应留下普通音符"
    );
    assert!(
        editor
            .drawn_shape_pending_at_screen(editor.line_pos_screen_pos((120.0, 62.0)))
            .is_none(),
        "Bake 后不得再命中图形/打开图形菜单"
    );

    assert!(editor.undo(), "Bake 应可撤销");
    assert_eq!(
        editor.editor_state.line_tool.paths.len(),
        1,
        "undo 应恢复待确认曲线"
    );
    assert!(
        editor.editor_state.data.current_track_notes().is_empty(),
        "undo 应删除 Bake 生成的音符"
    );

    assert!(editor.redo(), "Bake 应可重做");
    assert!(
        editor.editor_state.line_tool.paths.is_empty(),
        "redo 应再次丢弃曲线"
    );
    assert!(
        !editor.editor_state.data.current_track_notes().is_empty(),
        "redo 应重新生成音符"
    );
}

#[test]
fn test_record_brush_strokes_keeps_base_track_and_notes() {
    let mut editor = test_editor();
    editor.editor_state.brush_tool.strokes.push(BrushStroke {
        points: vec![(0.0, 60.0), (4.0, 62.0)],
        base_track: 1,
    });
    let notes = vec![vec![ShapeNote {
        track: 1,
        tick: 0.0,
        key: 60,
        length: 1.0,
        velocity: 100,
        channel: 0,
    }]];
    editor.record_brush_strokes(notes);
    let st = &editor.editor_state.shape_select;
    assert_eq!(st.len(), 1);
    assert_eq!(st.shapes()[0].track, 1, "应登记在落笔基准轨");
    assert_eq!(st.shapes()[0].notes.len(), 1);
    assert!(matches!(
        st.shapes()[0].source,
        DrawnShapeSource::Polyline { .. }
    ));
}

// ── 点选 ─────────────────────────────────────────────

#[test]
fn test_context_menu_target_distinguishes_pending_and_confirmed() {
    let mut pending = test_editor();
    draw_pending_rect(&mut pending);
    pending.set_tool(Tool::ShapeSelect);
    let pending_pos = pending.line_pos_screen_pos((2.0, 62.0));
    assert_eq!(
        pending.drawn_shape_pending_at_screen(pending_pos),
        Some(true),
        "pending 镜像必须走 PendingDrawnShape 目标"
    );

    let mut confirmed = draw_and_confirm_rect();
    confirmed.set_tool(Tool::ShapeSelect);
    let confirmed_pos = confirmed.line_pos_screen_pos((2.0, 62.0));
    assert_eq!(
        confirmed.drawn_shape_pending_at_screen(confirmed_pos),
        Some(false),
        "已确认图形才允许 DrawnShape 目标"
    );
}

#[test]
fn test_pending_menu_allowed_follows_owner_tool() {
    let mut editor = test_editor();
    draw_pending_rect(&mut editor);

    editor.set_tool(Tool::Curve);
    assert!(
        !editor.pending_menu_allowed(PendingShapeRef::CurvePath(0)),
        "曲线工具不承载组合预览，应在 ShapeSelect 下组合"
    );

    editor.set_tool(Tool::Shape);
    assert!(
        editor.pending_menu_allowed(PendingShapeRef::Shape(0)),
        "形状工具应能直接右键自己的 pending"
    );

    editor.set_tool(Tool::ShapeSelect);
    assert!(
        editor.pending_menu_allowed(PendingShapeRef::Composed(0)),
        "选择工具对所有 pending 开放"
    );

    editor.set_tool(Tool::Pencil);
    assert!(
        !editor.pending_menu_allowed(PendingShapeRef::CurvePath(0)),
        "非 owner 工具不得开 pending 菜单"
    );
}

#[test]
fn test_mouse_tool_selects_inside_and_deselects_outside() {
    let mut editor = draw_and_confirm_rect();
    editor.set_tool(Tool::ShapeSelect);
    editor.handle_shape_select_pressed(2.0, 62.0, false);
    assert!(
        editor.editor_state.shape_select.selection_len() > 0,
        "点在图形内部应选中"
    );
    editor.handle_shape_select_pressed(900.0, 10.0, false);
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        0,
        "点空白应取消选中"
    );
}

#[test]
fn test_circle_hit_uses_ellipse_interior() {
    let mut editor = test_editor();
    editor.set_shape(ShapeKind::Circle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool());
    register_test_drawn_shape(
        &mut editor,
        DrawnShapeSource::Shape {
            kind: ShapeKind::Circle,
            rect: (0.0, 60.0, 4.0, 64.0),
            shift_constrained: false,
            filled: false,
            apex_high: false,
        },
    );
    editor.set_tool(Tool::ShapeSelect);
    // 命中测试口径：椭圆内部，而非外接矩形
    assert_eq!(
        editor.hit_test_drawn_shape(2.0, 62.0),
        Some(0),
        "圆心应命中"
    );
    assert_eq!(
        editor.hit_test_drawn_shape(0.05, 60.05),
        None,
        "外接矩形角落（椭圆之外）不应命中"
    );
    editor.handle_shape_select_pressed(2.0, 62.0, false);
    assert!(editor.editor_state.shape_select.selection_len() > 0);
}

#[test]
fn test_other_track_shape_not_hit() {
    let mut editor = draw_and_confirm_rect();
    editor.set_tool(Tool::ShapeSelect);
    editor.editor_state.data.current_track = 2;
    editor.handle_shape_select_pressed(2.0, 62.0, false);
    assert_eq!(editor.editor_state.shape_select.selection_len(), 0);
}

// ── 移动 ─────────────────────────────────────────────

#[test]
fn test_drag_moves_shape_geometry_and_notes() {
    let mut editor = draw_and_confirm_rect();
    editor.set_tool(Tool::ShapeSelect);
    // 按下图形内一点并拖到 (+4 tick, +2 key)
    editor.handle_shape_select_pressed(2.0, 62.0, false);
    editor.handle_shape_select_moved(6.0, 64.0);
    editor.handle_shape_select_released();

    let st = &editor.editor_state.shape_select;
    match &st.shapes()[0].source {
        DrawnShapeSource::Shape { rect, .. } => {
            assert_eq!(*rect, (4.0, 62.0, 8.0, 66.0), "几何应整体平移 (+4, +2)")
        }
        other => panic!("期望 Shape，实际 {other:?}"),
    }
    assert!(st.shapes()[0].notes.iter().all(|n| n.key >= 62));
    assert_eq!(st.shapes()[0].moves.len(), 1, "应记录一次移动");
    // 文档音符随之平移：数量不变、key 全部 >= 62、最小起点 = 4
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        RECT_OUTLINE_NOTES
    );
    assert!(note_keys(&editor).iter().all(|&k| k >= 62));
    assert_eq!(note_ticks(&editor).iter().copied().min(), Some(4));
}

#[test]
fn test_undo_move_restores_geometry_and_notes() {
    let mut editor = draw_and_confirm_rect();
    editor.set_tool(Tool::ShapeSelect);
    editor.handle_shape_select_pressed(2.0, 62.0, false);
    editor.handle_shape_select_moved(6.0, 64.0);
    editor.handle_shape_select_released();
    assert!(editor.undo(), "应能撤销移动");
    match &editor.editor_state.shape_select.shapes()[0].source {
        DrawnShapeSource::Shape { rect, .. } => {
            assert_eq!(*rect, (0.0, 60.0, 4.0, 64.0), "撤销后几何应复原")
        }
        other => panic!("期望 Shape，实际 {other:?}"),
    }
    assert!(note_keys(&editor).contains(&60), "撤销后音符应回原位");
    assert_eq!(note_ticks(&editor).iter().copied().min(), Some(0));
}

#[test]
fn test_drag_without_movement_creates_no_history() {
    let mut editor = draw_and_confirm_rect();
    editor.set_tool(Tool::ShapeSelect);
    editor.handle_shape_select_pressed(2.0, 62.0, false);
    // 未超过一格：delta 归零
    editor.handle_shape_select_moved(2.2, 62.0);
    editor.handle_shape_select_released();
    assert!(
        editor.editor_state.shape_select.shapes()[0]
            .moves
            .is_empty()
    );
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        RECT_OUTLINE_NOTES
    );
}

// ── 删除 ─────────────────────────────────────────────

#[test]
fn test_delete_shape_removes_its_notes_and_object() {
    let mut editor = draw_and_confirm_rect();
    editor.set_tool(Tool::ShapeSelect);
    editor.handle_shape_select_pressed(2.0, 62.0, false);
    assert!(editor.delete_selected_drawn_shape());
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "删除图形应连带删除其音符"
    );
    assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 0);
    assert_eq!(editor.editor_state.shape_select.selection_len(), 0);
}

#[test]
fn test_undo_delete_restores_notes_and_shape() {
    let mut editor = draw_and_confirm_rect();
    editor.set_tool(Tool::ShapeSelect);
    editor.handle_shape_select_pressed(2.0, 62.0, false);
    assert!(editor.delete_selected_drawn_shape());
    assert!(editor.undo(), "应能撤销删除");
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        RECT_OUTLINE_NOTES,
        "撤销删除应恢复音符"
    );
    assert_eq!(
        editor.editor_state.shape_select.visible_on(1).count(),
        1,
        "撤销删除应恢复图形可见性"
    );
    assert!(editor.redo(), "应能重做删除");
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
    assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 0);
}

#[test]
fn test_delete_without_notes_still_removes_object() {
    let mut editor = draw_and_confirm_rect();
    editor.set_tool(Tool::ShapeSelect);
    editor.handle_shape_select_pressed(2.0, 62.0, false);
    // 先手动清空该轨音符（模拟用户已先删掉音符）
    editor.editor_state.data.document = None;
    assert!(
        editor.delete_selected_drawn_shape(),
        "无音符时应仍收掉图形对象"
    );
    assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 0);
}

// ── 历史同步 ──────────────────────────────────────────

#[test]
fn test_undo_creation_hides_shape_and_redo_restores() {
    let mut editor = draw_and_confirm_rect();
    assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 1);
    assert!(editor.undo(), "撤销音符创建应成功");
    assert_eq!(
        editor.editor_state.shape_select.visible_on(1).count(),
        0,
        "撤销后图形应隐藏（避免幽灵描边）"
    );
    assert!(editor.redo(), "重做应成功");
    assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 1);
}

#[test]
fn test_undo_bake_restores_pending_shape_hit() {
    let mut editor = draw_and_confirm_rect();
    editor.set_tool(Tool::ShapeSelect);
    editor.handle_shape_select_pressed(2.0, 62.0, false);
    // ??????????????????????
    editor.handle_shape_select_released();
    assert!(editor.editor_state.shape_select.selection_len() > 0);
    assert!(editor.undo());
    assert!(
        editor.editor_state.shape_tool.has_pending(),
        "undo Bake ????????"
    );
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "undo ??? Bake ?????"
    );
    assert!(
        editor.hit_test_drawn_shape(2.0, 62.0).is_some(),
        "???????????????"
    );
}

#[test]

fn test_reset_clears_registry() {
    let mut editor = draw_and_confirm_rect();
    assert!(!editor.editor_state.shape_select.is_empty());
    editor.editor_state.reset();
    assert!(
        editor.editor_state.shape_select.is_empty(),
        "reset 应清空图形注册表"
    );
}

#[test]
fn test_point_segment_distance_basics() {
    let d = point_segment_distance((5.0, 3.0), (0.0, 0.0), (10.0, 0.0));
    assert!((d - 3.0).abs() < 1e-4, "期望 3.0，实际 {d}");
    let d = point_segment_distance((14.0, 0.0), (0.0, 0.0), (10.0, 0.0));
    assert!((d - 4.0).abs() < 1e-4, "期望 4.0，实际 {d}");
}

// ── 切换到鼠标工具：产物保留、不生成音符（回归） ─────────

/// 拉出（但不 √）一个待确认形状：模拟「用户刚画完图案还没确认」的状态
fn draw_pending_rect(editor: &mut Editor) {
    editor.set_tool(Tool::Shape);
    editor.set_shape(ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(
        editor.editor_state.shape_tool.has_pending(),
        "前置条件：应存在待确认图形"
    );
}

#[test]
fn test_switch_to_mouse_tool_keeps_pending_shape_without_notes() {
    let mut editor = test_editor();
    draw_pending_rect(&mut editor);
    editor.set_tool(Tool::ShapeSelect);
    // ① 切换工具**不得**生成音符：用户没按 √，音符就不该落地
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "切换到鼠标工具不得直接生成音符"
    );
    // ② 待确认图形（图案）必须保留
    assert!(
        editor.editor_state.shape_tool.has_pending(),
        "切换工具不得丢弃待确认图形"
    );
    // ③ 切到鼠标工具：待确认几何**进入图形选中域**（镜像），但**不带音符**——
    //    「没按 √ 就没有音符」与「看得见就必须框得中」两条同时成立。
    //    （BUG 历史：只保留不登记 ⇒ 用户看到图案却框不中、拖不动、无选框。）
    assert_eq!(
        editor.editor_state.shape_select.len(),
        1,
        "待确认图形应登记为选中域镜像（否则鼠标工具下框不中）"
    );
    let mirror = &editor.editor_state.shape_select.shapes()[0];
    assert!(
        mirror.pending.is_some(),
        "该条目必须是待确认镜像（来源 = 形状工具的待确认图形）"
    );
    assert!(
        mirror.notes.is_empty(),
        "镜像不得携带音符：音符只属于 √ 确认"
    );
    assert!(
        mirror.source.bounds().is_some(),
        "镜像必须有几何（可供框选命中与画选框）"
    );
    assert!(
        editor.selection_box().is_none(),
        "尚未框选 / 点选时不该直接出现选框"
    );
    // ④ 离开鼠标工具：镜像撤掉（其余工具由 owning 预览渲染，不该留选中域条目）
    editor.set_tool(Tool::Pencil);
    assert!(
        editor.editor_state.shape_select.is_empty(),
        "离开鼠标工具应撤掉镜像"
    );
    // ⑤ 切回形状工具仍能 √ 固化 —— 图案没被毁
    editor.set_tool(Tool::Shape);
    assert!(editor.confirm_shape_tool(), "切回后仍可 √ 固化");
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        RECT_OUTLINE_NOTES
    );
    assert_eq!(
        editor.editor_state.shape_select.visible_on(1).count(),
        0,
        "确认后 Bake，不应保留可交互图形"
    );
}

#[test]
fn test_switch_to_mouse_tool_keeps_pending_curve_paths() {
    let mut editor = test_editor();
    editor.set_tool(Tool::Curve);
    editor.editor_state.line_tool.paths = vec![vec![
        BezierAnchor::new((0.0, 60.0)),
        BezierAnchor::new((240.0, 64.0)),
    ]];
    editor.editor_state.line_tool.recompute_auto_handles();
    editor.set_tool(Tool::ShapeSelect);
    assert_eq!(
        editor.editor_state.line_tool.paths.len(),
        1,
        "切换工具不得丢弃待确认曲线路径"
    );
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
    // 切回曲线工具仍可 √ 生成音符
    editor.set_tool(Tool::Curve);
    assert!(editor.confirm_line_tool());
    assert!(editor.editor_state.data.current_track_note_count() > 0);
}

#[test]
fn test_switch_to_mouse_tool_keeps_pending_brush_strokes() {
    let mut editor = test_editor();
    editor.set_tool(Tool::Brush);
    // 粗细度 1：单层笔画全部落在基准轨，便于断言
    editor.brush.set_thickness(1);
    editor.editor_state.brush_tool.strokes.push(BrushStroke {
        points: vec![(0.0, 60.0), (1.0, 60.0), (2.0, 60.0)],
        base_track: 1,
    });
    editor.set_tool(Tool::ShapeSelect);
    assert_eq!(
        editor.editor_state.brush_tool.strokes.len(),
        1,
        "切换工具不得丢弃待确认笔画"
    );
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
    editor.set_tool(Tool::Brush);
    assert!(editor.confirm_brush());
    assert!(editor.editor_state.data.current_track_note_count() > 0);
}

#[test]
fn test_switch_to_any_tool_keeps_pending() {
    // 「其他工具同理」：切到任意工具都保留产物（清空只发生在显式 × / √）
    let mut editor = test_editor();
    draw_pending_rect(&mut editor);
    for tool in [
        Tool::Curve,
        Tool::Brush,
        Tool::Pencil,
        Tool::Text,
        Tool::Pointer,
    ] {
        editor.set_tool(tool);
        assert!(
            editor.editor_state.shape_tool.has_pending(),
            "切到 {tool:?} 后待确认图形应保留"
        );
        assert_eq!(
            editor.editor_state.data.current_track_note_count(),
            0,
            "切到 {tool:?} 不得生成音符"
        );
    }
}

#[test]
fn test_switch_tool_cancels_interaction_but_keeps_artifacts() {
    // 手势（未完成的拖动）必须收敛；产物必须保留 —— 两者不可混为一谈
    let mut editor = test_editor();
    draw_pending_rect(&mut editor);
    editor.editor_state.shape_tool.interaction =
        ShapeToolInteraction::Dragging { start: (0.0, 60.0) };
    editor.set_tool(Tool::ShapeSelect);
    assert_eq!(
        editor.editor_state.shape_tool.interaction,
        ShapeToolInteraction::None,
        "切换工具应收敛未完成的交互手势"
    );
    assert!(
        editor.editor_state.shape_tool.has_pending(),
        "但待确认图形必须留着"
    );
}

#[test]
fn test_switch_tool_cancels_shape_drag_without_moving_it() {
    // 拖动已绘制图形途中切走：预览偏移丢弃（几何/音符不动），
    // 拖拽态必须收敛，否则 is_editing() 永久 true → 撤销/重做被静默堵死
    let mut editor = draw_and_confirm_rect();
    editor.set_tool(Tool::ShapeSelect);
    editor.handle_shape_select_pressed(2.0, 62.0, false);
    editor.handle_shape_select_moved(6.0, 64.0);
    assert!(editor.editor_state.shape_select.is_dragging());
    editor.set_tool(Tool::Curve);
    assert!(
        !editor.editor_state.shape_select.is_dragging(),
        "换工具必须收敛拖拽态"
    );
    assert!(
        !editor.is_editing(),
        "拖拽态残留会让 is_editing() 永久为 true"
    );
    match &editor.editor_state.shape_select.shapes()[0].source {
        DrawnShapeSource::Shape { rect, .. } => {
            assert_eq!(*rect, (0.0, 60.0, 4.0, 64.0), "几何不应被平移")
        }
        other => panic!("期望 Shape，实际 {other:?}"),
    }
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        RECT_OUTLINE_NOTES
    );
}

#[test]
fn test_pending_preview_visible_rule() {
    let mut editor = test_editor();
    editor.set_tool(Tool::Shape);
    assert!(
        editor.pending_preview_visible(Tool::Shape),
        "拥有者工具下应可见"
    );
    assert!(
        !editor.pending_preview_visible(Tool::Curve),
        "非拥有者绘制工具下不应渲染"
    );
    editor.set_tool(Tool::ShapeSelect);
    for owner in [Tool::Curve, Tool::Shape, Tool::Brush, Tool::Text] {
        assert!(
            editor.pending_preview_visible(owner),
            "鼠标工具下 {owner:?} 的待确认预览应可见（图案不消失）"
        );
    }
    editor.set_tool(Tool::Pencil);
    assert!(
        !editor.pending_preview_visible(Tool::Curve),
        "铅笔工具下不渲染待确认预览（避免浮层干扰音符编辑）"
    );
}

// ── 空白拉框（框选） ──────────────────────────────────

#[test]
fn test_marquee_selects_shape_inside_box() {
    let mut editor = draw_and_confirm_rect();
    editor.set_tool(Tool::ShapeSelect);
    // 空白处按下（远离 0..4 × 60..64 的图形）→ 拉出覆盖图形的框
    editor.handle_shape_select_pressed(50.0, 80.0, false);
    assert!(
        editor.editor_state.shape_select.is_marqueeing(),
        "空白按下应进入拉框态"
    );
    editor.handle_shape_select_moved(0.0, 55.0);
    editor.handle_shape_select_released();
    assert_eq!(
        editor.editor_state.shape_select.selected_ids(),
        &[0],
        "框内图形应被框选选中"
    );
    assert!(
        !editor.editor_state.shape_select.is_marqueeing(),
        "松手后拉框态应清空"
    );
}

#[test]
fn test_marquee_outside_box_selects_nothing() {
    let mut editor = draw_and_confirm_rect();
    editor.set_tool(Tool::ShapeSelect);
    editor.handle_shape_select_pressed(2.0, 62.0, false); // 先选中图形
    assert!(editor.editor_state.shape_select.selection_len() > 0);
    // 空白处拉一个与图形不相交的框 → 取消选中
    editor.handle_shape_select_pressed(50.0, 80.0, false);
    editor.handle_shape_select_moved(60.0, 90.0);
    editor.handle_shape_select_released();
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        0,
        "框外无图形应保持未选中"
    );
}

#[test]
fn test_tiny_drag_is_click_not_marquee() {
    let mut editor = draw_and_confirm_rect();
    editor.set_tool(Tool::ShapeSelect);
    editor.handle_shape_select_pressed(2.0, 62.0, false);
    assert!(editor.editor_state.shape_select.selection_len() > 0);
    // 空白处「点一下」（无实际位移）→ 仍是「点空白取消选中」，不构成框选
    editor.handle_shape_select_pressed(50.0, 80.0, false);
    editor.handle_shape_select_moved(50.0, 80.0);
    editor.handle_shape_select_released();
    assert_eq!(editor.editor_state.shape_select.selection_len(), 0);
}

#[test]
fn test_marquee_selects_all_shapes_inside_box() {
    // 多选语义：框选把框内**全部**图形一并选中（不再只取最上层）
    let mut editor = draw_and_confirm_rect();
    // 再登记一个不重叠的远侧矩形
    let far = editor.editor_state.shape_select.add(
        1,
        None,
        DrawnShapeSource::Shape {
            kind: ShapeKind::Rectangle,
            rect: (200.0, 60.0, 204.0, 64.0),
            shift_constrained: false,
            apex_high: true,
            filled: false,
        },
        Vec::new(),
    );
    editor.set_tool(Tool::ShapeSelect);
    // 拉一个同时覆盖两个图形的框
    editor.handle_shape_select_pressed(300.0, 100.0, false);
    editor.handle_shape_select_moved(0.0, 50.0);
    editor.handle_shape_select_released();
    assert_eq!(
        editor.editor_state.shape_select.selected_ids(),
        &[0, far],
        "框内全部图形都应被选中（按登记顺序）"
    );
}

#[test]
fn test_marquee_selects_two_pending_mirrors() {
    // S4 前置：两个待确认形状必须能被一次框选同时选中
    let mut editor = Editor::new();
    editor.editor_state.data.current_track = 1;
    editor.set_tool(Tool::ShapeSelect);
    for (t0, t1) in [(0.0_f32, 10.0_f32), (100.0, 110.0)] {
        editor
            .editor_state
            .shape_tool
            .shapes
            .push(lumino_editor_state::ShapeInstance {
                kind: lumino_editor_state::ShapeKind::Rectangle,
                rect: (t0, 60.0, t1, 64.0),
                shift_constrained: false,
                filled: true,
                apex_high: true,
                mask: None,
            });
    }
    editor.rebuild_pending_mirrors();
    assert_eq!(
        editor.editor_state.shape_select.pending_shapes().count(),
        2,
        "应有两个待确认镜像"
    );

    editor.handle_shape_select_pressed(200.0, 80.0, false);
    editor.handle_shape_select_moved(-10.0, 55.0);
    editor.handle_shape_select_released();
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        2,
        "框选应同时选中两个待确认镜像"
    );
}

#[test]
fn test_marquee_ignores_hidden_shape_and_other_track() {
    let mut editor = draw_and_confirm_rect();
    editor.set_tool(Tool::ShapeSelect);
    // 切到别的音轨：当前轨无图形 → 框选不应选中任何东西
    editor.editor_state.data.current_track = 2;
    editor.handle_shape_select_pressed(50.0, 80.0, false);
    editor.handle_shape_select_moved(0.0, 55.0);
    editor.handle_shape_select_released();
    assert_eq!(editor.editor_state.shape_select.selection_len(), 0);
}

// ── 选中选框 + 框内拖动（统一移动方式） ─────────────────

/// 一条大 L 形折线（逻辑 0..500 × 60..80）+ 与之匹配的文档音符，
/// 并已完成选中。选框内部有大量「远离折线」的空白，用于验证「框内拖动」。
fn editor_with_selected_polyline() -> Editor {
    let mut editor = Editor::new();
    seed_notes(
        &mut editor,
        2,
        1,
        &[
            Note::new(0.0, 60, 1.0),
            Note::new(500.0, 60, 1.0),
            Note::new(500.0, 80, 1.0),
        ],
    );
    editor.editor_state.view.snap_precision = 1.0;
    let points = vec![(0.0, 60.0), (500.0, 60.0), (500.0, 80.0)];
    let notes = vec![
        ShapeNote {
            track: 1,
            tick: 0.0,
            key: 60,
            length: 1.0,
            velocity: 100,
            channel: 0,
        },
        ShapeNote {
            track: 1,
            tick: 500.0,
            key: 60,
            length: 1.0,
            velocity: 100,
            channel: 0,
        },
        ShapeNote {
            track: 1,
            tick: 500.0,
            key: 80,
            length: 1.0,
            velocity: 100,
            channel: 0,
        },
    ];
    let id =
        editor
            .editor_state
            .shape_select
            .add(1, None, DrawnShapeSource::Polyline { points }, notes);
    editor.set_tool(Tool::ShapeSelect);
    editor.editor_state.shape_select.select_only(id);
    editor
}

#[test]
fn test_selection_box_exists_and_padding() {
    let editor = test_editor();
    assert!(editor.selection_box().is_none(), "无选中时不应有选框");

    let editor2 = editor_with_selected_polyline();
    let rect = editor2.selection_box().expect("选中后应有选框");
    assert!(rect.width > 0.0 && rect.height > 0.0, "选框必须有面积");
    // 折线逻辑 0..500 × 60..80 → 屏幕外接框按 padding 外扩
    let a = editor2.line_pos_screen_pos((0.0, 60.0));
    let b = editor2.line_pos_screen_pos((500.0, 80.0));
    let min_x = a.x.min(b.x) - SHAPE_BOX_PADDING_PX;
    assert!(
        (rect.position().x - min_x).abs() < 1e-3,
        "选框左边界应为几何外接框左边界外扩 padding"
    );

    // 图形属 track 1，切到 track 2 后不应再有选框
    let mut editor3 = editor_with_selected_polyline();
    editor3.editor_state.data.current_track = 2;
    assert!(
        editor3.selection_box().is_none(),
        "图形不属当前音轨时不应有选框"
    );
}

#[test]
fn test_press_inside_selection_box_keeps_selection_and_start_drag() {
    let mut editor = editor_with_selected_polyline();
    // (250, 70) 位于折线选框内部，但远离折线本身（点选命中应为 None）
    assert_eq!(editor.hit_test_drawn_shape(250.0, 70.0), None);
    assert!(editor.point_in_selection_box(250.0, 70.0), "该点应在选框内");
    let id = editor.editor_state.shape_select.selected_ids().to_vec();
    editor.handle_shape_select_pressed(250.0, 70.0, false);
    assert_eq!(
        editor.editor_state.shape_select.selected_ids(),
        &id[..],
        "点选框内部不应取消选中"
    );
    assert!(editor.editor_state.shape_select.is_dragging(), "应进入拖动");
    // 未移动即松手：不产生移动，选中保持
    editor.handle_shape_select_released();
    assert_eq!(editor.editor_state.shape_select.selected_ids(), &id[..]);
    assert!(
        editor.editor_state.shape_select.shapes()[0]
            .moves
            .is_empty()
    );
}

#[test]
fn test_drag_inside_selection_box_moves_polyline_and_notes() {
    let mut editor = editor_with_selected_polyline();
    // 框内空白处按下并拖动 (+10 tick, +2 key)
    editor.handle_shape_select_pressed(250.0, 70.0, false);
    editor.handle_shape_select_moved(260.0, 72.0);
    editor.handle_shape_select_released();

    match &editor.editor_state.shape_select.shapes()[0].source {
        DrawnShapeSource::Polyline { points } => assert_eq!(
            points,
            &vec![(10.0, 62.0), (510.0, 62.0), (510.0, 82.0)],
            "折线几何应整体平移 (+10, +2)"
        ),
        other => panic!("期望 Polyline，实际 {other:?}"),
    }
    assert_eq!(
        editor.editor_state.shape_select.shapes()[0].moves.len(),
        1,
        "应记录一次移动"
    );
    // 文档音符随之平移
    let mut keys: Vec<u16> = note_keys(&editor);
    keys.sort_unstable();
    assert_eq!(keys, vec![62, 62, 82], "音符应整体上移 2 个半音");
    let mut ticks: Vec<u32> = note_ticks(&editor);
    ticks.sort_unstable();
    assert_eq!(ticks, vec![10, 510, 510], "音符应整体右移 10 tick");

    // 撤销 → 几何与音符复原
    assert!(editor.undo(), "应能撤销框内拖动产生的移动");
    match &editor.editor_state.shape_select.shapes()[0].source {
        DrawnShapeSource::Polyline { points } => assert_eq!(
            points,
            &vec![(0.0, 60.0), (500.0, 60.0), (500.0, 80.0)],
            "撤销后几何应复原"
        ),
        other => panic!("期望 Polyline，实际 {other:?}"),
    }
    assert!(note_keys(&editor).contains(&60));
}

#[test]
fn test_point_hit_takes_priority_over_selection_box() {
    let mut editor = editor_with_selected_polyline();
    let poly_id = editor
        .editor_state
        .shape_select
        .selected_ids()
        .first()
        .copied()
        .expect("前置：折线已选中");
    // 在折线选框内部再登记一个小矩形（后登记 = 最上层）
    let rect_id = editor.editor_state.shape_select.add(
        1,
        None,
        DrawnShapeSource::Shape {
            kind: ShapeKind::Rectangle,
            rect: (240.0, 68.0, 260.0, 72.0),
            shift_constrained: false,
            apex_high: true,
            filled: false,
        },
        Vec::new(),
    );
    assert!(
        editor.point_in_selection_box(250.0, 70.0),
        "前置：该点确在折线选框内"
    );
    editor.handle_shape_select_pressed(250.0, 70.0, false);
    assert_eq!(
        editor.editor_state.shape_select.selected_ids(),
        &[rect_id],
        "点中图形应优先于「选框内部拖动」（否则无法选中重叠的其它图形）"
    );
    assert!(
        editor.editor_state.shape_select.is_dragging(),
        "拖动应落在被命中的那个图形上（选中集已替换为它）"
    );
    editor.handle_shape_select_released();
    assert_eq!(editor.editor_state.shape_select.selected_ids(), &[rect_id]);
    assert_ne!(rect_id, poly_id, "两个图形应各有独立 ID");
}

#[test]
fn test_press_outside_selection_box_clears_and_starts_marquee() {
    let mut editor = editor_with_selected_polyline();
    // (900, 10) 落在选框外 → 取消选中并起框
    assert!(!editor.point_in_selection_box(900.0, 10.0));
    editor.handle_shape_select_pressed(900.0, 10.0, false);
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        0,
        "选框外按下应取消选中"
    );
    assert!(
        editor.editor_state.shape_select.is_marqueeing(),
        "选框外按下应起框"
    );
    assert!(!editor.editor_state.shape_select.is_dragging());
    editor.handle_shape_select_released();
}

#[test]
fn test_selection_box_follows_drag_preview_offset() {
    let mut editor = editor_with_selected_polyline();
    let base = editor.selection_box().expect("应有选框");
    editor.handle_shape_select_pressed(250.0, 70.0, false);
    editor.handle_shape_select_moved(260.0, 72.0);
    let dragged = editor.selection_box().expect("拖拽中仍应有选框");
    // 拖拽预览：选框应随预览偏移移动（未落文档）
    let dx = dragged.position().x - base.position().x;
    let dy = dragged.position().y - base.position().y;
    assert!(
        (dx - 10.0 * editor.editor_state.view.zoom_x).abs() < 1e-3,
        "选框 X 应跟随预览偏移，实际 dx={dx}"
    );
    assert!(
        (dy + 2.0 * editor.editor_state.view.zoom_y).abs() < 1e-3,
        "选框 Y 应跟随预览偏移（key 增大 y 减小），实际 dy={dy}"
    );
    editor.handle_shape_select_released();
}

#[test]
fn test_press_just_outside_box_top_is_not_grabbed() {
    // 真实按下链路（`handle_tool_pressed`）必须用**未取整**的原始 key：
    // 选框命中区精细到 3px，若按键位整数化（zoom_y=20 时 1 key = 20px），
    // 框外 5px 的点会被量化进框内、被误判成「框内拖动」而不是起框。
    let mut editor = editor_with_selected_polyline();
    let top_y = editor.line_pos_screen_pos((0.0, 80.0)).y - SHAPE_BOX_PADDING_PX;
    let x = editor.line_pos_screen_pos((250.0, 70.0)).x;
    let pos = lumino_ui_core::geom::Point::new(x, top_y - 5.0);
    let snapped = editor.snap_tick(editor.pos_to_tick(pos));
    editor.handle_tool_pressed(pos, false, snapped, 0);
    assert!(
        editor.editor_state.shape_select.is_marqueeing(),
        "框外按下应起框，而非被整数 key 量化误判为框内拖动"
    );
    assert_eq!(editor.editor_state.shape_select.selection_len(), 0);
    assert!(!editor.editor_state.shape_select.is_dragging());
}

// ── 多选（框选多个 / Shift 增删 / 整组移动 · 删除） ──────

/// 两条带音符的水平折线（track 1）+ 匹配的文档音符，且**已全部选中**
///
/// A：key 60 的横线（0..300）；B：key 70 的横线（0..300），共 4 条文档音符。
/// 两条线相距 10 个半音 → 选框并集 0..300 × 60..70，中间有大片空白可拖。
fn editor_with_two_selected_polylines() -> Editor {
    let mut editor = Editor::new();
    seed_notes(
        &mut editor,
        2,
        1,
        &[
            Note::new(0.0, 60, 1.0),
            Note::new(100.0, 60, 1.0),
            Note::new(0.0, 70, 1.0),
            Note::new(100.0, 70, 1.0),
        ],
    );
    editor.editor_state.view.snap_precision = 1.0;
    let line = |key: u16| DrawnShapeSource::Polyline {
        points: vec![(0.0, key as f32), (300.0, key as f32)],
    };
    let notes = |key: u16| {
        vec![
            ShapeNote {
                track: 1,
                tick: 0.0,
                key,
                length: 1.0,
                velocity: 100,
                channel: 0,
            },
            ShapeNote {
                track: 1,
                tick: 100.0,
                key,
                length: 1.0,
                velocity: 100,
                channel: 0,
            },
        ]
    };
    let a = editor
        .editor_state
        .shape_select
        .add(1, None, line(60), notes(60));
    let b = editor
        .editor_state
        .shape_select
        .add(1, None, line(70), notes(70));
    editor.set_tool(Tool::ShapeSelect);
    editor.editor_state.shape_select.select_all_of([a, b]);
    editor
}

#[test]
fn test_shift_click_toggles_selection_membership() {
    let mut editor = editor_with_two_selected_polylines();
    let ids: Vec<u64> = editor.editor_state.shape_select.selected_ids().to_vec();
    assert_eq!(ids.len(), 2);
    // Shift 点中**已选中**的成员 → 从选中集移出，且不进入拖拽
    editor.handle_shape_select_pressed(150.0, 60.0, true);
    assert_eq!(
        editor.editor_state.shape_select.selected_ids(),
        &ids[1..],
        "Shift 点击已选中者应把它移出选中集"
    );
    assert!(
        !editor.editor_state.shape_select.is_dragging(),
        "移出后不应进入拖拽"
    );
    // 再 Shift 点它 → 加回（追加到末尾）
    editor.handle_shape_select_pressed(150.0, 60.0, true);
    assert_eq!(
        editor.editor_state.shape_select.selected_ids(),
        &[ids[1], ids[0]],
        "再次 Shift 应加回选中集"
    );
    assert!(editor.editor_state.shape_select.is_dragging());
    editor.handle_shape_select_released();
}

#[test]
fn test_shift_marquee_unions_instead_of_replacing() {
    let mut editor = editor_with_two_selected_polylines();
    // 先只选中第一条（清空选中集后点它）
    editor.editor_state.shape_select.clear_selection();
    editor.handle_shape_select_pressed(150.0, 60.0, false);
    assert_eq!(editor.editor_state.shape_select.selection_len(), 1);
    // Shift 拉框覆盖两条线 → 与既有选中集求**并集**
    editor.handle_shape_select_pressed(0.0, 58.0, true);
    editor.handle_shape_select_moved(300.0, 72.0);
    editor.handle_shape_select_released();
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        2,
        "Shift 框选应并入既有选中集"
    );
    // 普通拉框（不按 Shift）→ **替换**选中集
    editor.handle_shape_select_pressed(0.0, 58.0, false);
    editor.handle_shape_select_moved(300.0, 62.0);
    editor.handle_shape_select_released();
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        1,
        "普通框选应替换选中集"
    );
}

#[test]
fn test_click_on_selected_member_drags_whole_group() {
    let mut editor = editor_with_two_selected_polylines();
    let ids: Vec<u64> = editor.editor_state.shape_select.selected_ids().to_vec();
    // 点中**已选中**的成员：选中集不变 → 拖动即整组移动
    editor.handle_shape_select_pressed(150.0, 60.0, false);
    assert_eq!(
        editor.editor_state.shape_select.selected_ids(),
        &ids[..],
        "点已选中的成员不应缩小选中集"
    );
    editor.handle_shape_select_moved(160.0, 62.0);
    editor.handle_shape_select_released();
    let st = &editor.editor_state.shape_select;
    for (i, expect_key) in [(0usize, 62.0f32), (1usize, 72.0f32)] {
        match &st.shapes()[i].source {
            DrawnShapeSource::Polyline { points } => {
                assert_eq!(points[0], (10.0, expect_key), "第 {i} 条线应整组平移");
                assert_eq!(points[1], (310.0, expect_key));
            }
            other => panic!("期望 Polyline，实际 {other:?}"),
        }
        assert_eq!(st.shapes()[i].moves.len(), 1, "应记录一次移动");
    }
    let mut keys: Vec<u16> = note_keys(&editor);
    keys.sort_unstable();
    assert_eq!(keys, vec![62, 62, 72, 72], "两组音符都应上移 2 个半音");
}

#[test]
fn test_drag_inside_selection_box_moves_whole_selection() {
    let mut editor = editor_with_two_selected_polylines();
    // (150, 65) 位于两条线之间：不命中任何图形，但在并集选框内部
    assert_eq!(
        editor.hit_test_drawn_shape(150.0, 65.0),
        None,
        "前置：并集中间是空白"
    );
    assert!(editor.point_in_selection_box(150.0, 65.0));
    editor.handle_shape_select_pressed(150.0, 65.0, false);
    editor.handle_shape_select_moved(160.0, 67.0);
    editor.handle_shape_select_released();

    let mut ticks: Vec<u32> = note_ticks(&editor);
    ticks.sort_unstable();
    assert_eq!(ticks, vec![10, 10, 110, 110], "整组音符应右移 10 tick");
    let mut keys: Vec<u16> = note_keys(&editor);
    keys.sort_unstable();
    assert_eq!(keys, vec![62, 62, 72, 72]);
    // 整组共用同一历史分组 → 一次撤销全回来
    assert!(editor.undo(), "应能撤销整组移动");
    let mut keys: Vec<u16> = note_keys(&editor);
    keys.sort_unstable();
    assert_eq!(keys, vec![60, 60, 70, 70], "撤销应整组还原");
    match &editor.editor_state.shape_select.shapes()[0].source {
        DrawnShapeSource::Polyline { points } => {
            assert_eq!(points[0], (0.0, 60.0), "撤销后几何应复原")
        }
        other => panic!("期望 Polyline，实际 {other:?}"),
    }
}

#[test]
fn test_delete_removes_whole_selection() {
    let mut editor = editor_with_two_selected_polylines();
    assert!(editor.delete_selected_drawn_shape());
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "删除应连带删掉整组的全部音符"
    );
    assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 0);
    assert_eq!(editor.editor_state.shape_select.selection_len(), 0);
    // 一次快照 → 一次撤销恢复整组
    assert!(editor.undo(), "一次撤销应恢复整组");
    assert_eq!(editor.editor_state.data.current_track_note_count(), 4);
    assert_eq!(editor.editor_state.shape_select.visible_on(1).count(), 2);
    assert!(editor.redo(), "重做应再次删除整组");
    assert_eq!(editor.editor_state.data.current_track_note_count(), 0);
}

#[test]
fn test_right_click_on_selected_member_keeps_multi_selection() {
    let mut editor = editor_with_two_selected_polylines();
    let ids: Vec<u64> = editor.editor_state.shape_select.selected_ids().to_vec();
    assert_eq!(ids.len(), 2);
    // 右键命中**已在选中集内**的成员 → 保持多选（菜单「删除」作用于整个选区）
    let id = editor
        .editor_state
        .shape_select
        .shapes()
        .first()
        .map(|s| s.id)
        .expect("应有图形");
    editor.select_drawn_shape(id);
    assert_eq!(
        editor.editor_state.shape_select.selected_ids(),
        &ids[..],
        "右键已选中成员不应缩小选中集"
    );
}
