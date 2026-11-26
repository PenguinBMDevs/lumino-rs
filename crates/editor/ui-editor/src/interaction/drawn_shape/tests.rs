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

mod tests_part2;
