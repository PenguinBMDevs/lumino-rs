//! 图形框选（音符画工具栏「鼠标工具」）**端到端**回归：真实输入链路 + 屏幕坐标
//!
//! 与 `interaction::drawn_shape` 的单元测试不同，这里**不直接调用**
//! `handle_shape_select_pressed/moved/released`，而是走真实入口
//! `Editor::handle_action(EditorAction::*)` + `line_pos_screen_pos` 换算的屏幕坐标，
//! 覆盖「工具切换 → 按下 → 移动 → 松手 → 渲染叠加层」的完整链路。
//!
//! 背景（BUG 回归）：用户在音符画工具栏画完曲线/图形后切到「鼠标工具」框选，
//! 出现「图形看起来在画布上，却框不中、没有选中选框、也拖不动」的问题。

use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use iced_core::Point;
use lumino_message::Tool;
use lumino_ui_core::message::{EditorAction, Point2};

/// 带画布尺寸的编辑器（`is_inside_canvas` 需要非零 canvas 尺寸），当前轨 = 1
fn editor_with_canvas() -> Editor {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        2,
        1,
        &[Note::new(0.0, 60, 1.0), Note::new(4.0, 64, 1.0)],
    );
    editor.editor_state.view.snap_precision = 1.0;
    // 画布给足尺寸：全部 0..=127 键与测试用的 tick 都落在有效区内，
    // 避免 `is_inside_canvas` 因默认 zoom/keyboard_width 换算把按下点判成画布外。
    editor.set_canvas_size(Point::new(4000.0, 4000.0));
    editor.set_canvas_offset(Point::new(0.0, 0.0));
    editor
}

/// 同上，但当前轨**不含任何音符**：待确认产物的用例据此断言「拖动 / 删除都不生成音符」
fn editor_with_empty_canvas() -> Editor {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 2, 1, &[]);
    editor.editor_state.view.snap_precision = 1.0;
    editor.set_canvas_size(Point::new(4000.0, 4000.0));
    editor.set_canvas_offset(Point::new(0.0, 0.0));
    editor
}

/// 逻辑坐标 → 真实按下事件
fn press_at(editor: &mut Editor, logical: (f32, f32), shift: bool) {
    let p = editor.line_pos_screen_pos(logical);
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(p.x, p.y),
        shift,
        ctrl: false,
    });
}

/// 逻辑坐标 → 真实移动事件
fn move_to(editor: &mut Editor, logical: (f32, f32)) {
    let p = editor.line_pos_screen_pos(logical);
    editor.handle_action(EditorAction::Moved(Point2::new(p.x, p.y)));
}

/// 真实松手事件
fn release(editor: &mut Editor) {
    editor.handle_action(EditorAction::Released);
}

/// 用形状工具画出矩形并 √（真实确认链路，登记图形对象）
fn draw_and_confirm_rect(editor: &mut Editor) {
    editor.set_tool(Tool::Shape);
    editor.set_shape(lumino_editor_state::ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(editor.confirm_shape_tool(), "矩形应生成音符并登记图形");
}

/// 场景一：√ 确认后的图形 → 鼠标工具空白拉框 → **必须**出现选中选框，且框内可拖动
#[test]
fn test_marquee_over_confirmed_shape_shows_box_and_can_drag() {
    let mut editor = editor_with_canvas();
    draw_and_confirm_rect(&mut editor);
    editor.set_tool(Tool::ShapeSelect);

    // 空白处起框（右下 → 左上），框住 0..4 × 60..64 的矩形轮廓
    press_at(&mut editor, (50.0, 80.0), false);
    assert!(
        editor.editor_state.shape_select.is_marqueeing(),
        "空白按下应进入拉框态"
    );
    move_to(&mut editor, (0.0, 55.0));
    release(&mut editor);

    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        1,
        "框内图形应被框选选中"
    );
    let rect = editor
        .selection_box()
        .expect("框选后必须显示选中选框（BUG：框选后无选框）");
    for corner in [(0.0, 60.0), (4.0, 64.0)] {
        assert!(
            rect.contains(editor.line_pos_screen_pos(corner)),
            "选框应包住图形角点 {corner:?}"
        );
    }

    // 框内按下并拖动 (+4 tick, +2 key)
    press_at(&mut editor, (2.0, 62.0), false);
    assert!(
        editor.editor_state.shape_select.is_dragging(),
        "框选后应能从选框内起拖（BUG：框选后拖不动）"
    );
    move_to(&mut editor, (6.0, 64.0));
    release(&mut editor);

    match &editor.editor_state.shape_select.shapes()[0].source {
        lumino_editor_state::DrawnShapeSource::Shape { rect, .. } => {
            assert_eq!(*rect, (4.0, 62.0, 8.0, 66.0), "图形几何应整体平移 (+4,+2)")
        }
        other => panic!("期望 Shape 几何，实际 {other:?}"),
    }
    assert_eq!(
        editor.editor_state.shape_select.shapes()[0].moves.len(),
        1,
        "应记录一次移动"
    );
}

/// 场景二：√ 确认后的**曲线**（折线）→ 鼠标工具框选 → 选框 + 拖动
#[test]
fn test_marquee_over_confirmed_curve_shows_box_and_can_drag() {
    let mut editor = editor_with_canvas();
    editor.set_tool(Tool::Curve);
    editor.editor_state.line_tool.paths = vec![vec![
        lumino_editor_state::BezierAnchor::new((0.0, 60.0)),
        lumino_editor_state::BezierAnchor::new((40.0, 70.0)),
    ]];
    editor.editor_state.line_tool.recompute_auto_handles();
    assert!(editor.confirm_line_tool(), "曲线应生成音符并登记折线");
    editor.set_tool(Tool::ShapeSelect);

    press_at(&mut editor, (200.0, 90.0), false);
    move_to(&mut editor, (0.0, 40.0));
    release(&mut editor);

    assert!(
        editor.editor_state.shape_select.selection_len() > 0,
        "框内曲线应被框选选中"
    );
    assert!(
        editor.selection_box().is_some(),
        "框选曲线后必须显示选中选框（BUG：框选后无选框）"
    );
}

/// 场景三：**未 √** 的待确认图形 → 切「鼠标工具」→ 框选 → 选框 + 拖动 → √ 落在新位置
///
/// 语义：切换工具保留产物（不擅自生成音符，见 `EditorState::set_tool`），
/// 但产物必须**可选中 / 可拖动**，否则用户看到图案却框不中、拖不动。
#[test]
fn test_marquee_over_unconfirmed_shape_is_selectable() {
    let mut editor = editor_with_empty_canvas();
    editor.set_tool(Tool::Shape);
    editor.set_shape(lumino_editor_state::ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    editor.handle_shape_tool_pressed(0.0, 60.0, false);
    editor.handle_shape_tool_moved(4.0, 64.0);
    editor.handle_shape_tool_released();
    assert!(
        editor.editor_state.shape_tool.has_pending(),
        "前置：应有未确认图形"
    );

    editor.set_tool(Tool::ShapeSelect);
    assert!(
        editor.pending_preview_visible(Tool::Shape),
        "鼠标工具下待确认图形应可见（用户据此才去框选）"
    );
    press_at(&mut editor, (50.0, 80.0), false);
    move_to(&mut editor, (0.0, 55.0));
    release(&mut editor);

    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        1,
        "画布上可见的待确认图形必须能被框选选中"
    );
    assert!(
        editor.selection_box().is_some(),
        "框选待确认图形后必须显示选中选框"
    );

    // 框内拖动 (+4 tick, +2 key)：几何即时写回待确认容器，文档不动
    press_at(&mut editor, (2.0, 62.0), false);
    assert!(
        editor.editor_state.shape_select.is_dragging(),
        "框选待确认图形后应能起拖"
    );
    move_to(&mut editor, (6.0, 64.0));
    release(&mut editor);

    let pending_rect = editor.editor_state.shape_tool.shapes[0].rect;
    assert_eq!(
        pending_rect,
        (4.0, 62.0, 8.0, 66.0),
        "拖动应把待确认几何整体平移 (+4,+2)"
    );
    let mirror = &editor.editor_state.shape_select.shapes()[0];
    assert_eq!(
        mirror.source.bounds(),
        Some((4.0, 8.0, 62.0, 66.0)),
        "镜像几何必须与待确认几何同源（tick 4..8 × key 62..66；否则选框 / 描边与图案分叉）"
    );
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "拖动待确认产物不得生成音符"
    );

    // 切回形状工具 √：音符应落在**拖动后**的位置
    editor.set_tool(Tool::Shape);
    assert!(editor.confirm_shape_tool(), "切回后仍可 √ 固化");
    let ticks: Vec<u32> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .map(|n| n.start_tick)
        .collect();
    assert_eq!(
        ticks.iter().copied().min(),
        Some(4),
        "√ 生成的音符应落在拖动后的位置（tick ≥ 4）"
    );
    assert!(
        editor
            .editor_state
            .data
            .current_track_notes()
            .iter()
            .all(|n| n.key >= 62),
        "√ 生成的音符 key 应整体上移 2"
    );
}

/// 场景三之二：**未 √** 的曲线路径 → 鼠标工具框选 → 拖动 → 曲线锚点被平移
#[test]
fn test_marquee_over_unconfirmed_curve_drags_anchors() {
    let mut editor = editor_with_empty_canvas();
    editor.set_tool(Tool::Curve);
    editor.editor_state.line_tool.paths = vec![vec![
        lumino_editor_state::BezierAnchor::new((0.0, 60.0)),
        lumino_editor_state::BezierAnchor::new((40.0, 70.0)),
    ]];
    editor.editor_state.line_tool.recompute_auto_handles();
    editor.set_tool(Tool::ShapeSelect);

    press_at(&mut editor, (200.0, 90.0), false);
    move_to(&mut editor, (0.0, 40.0));
    release(&mut editor);
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        1,
        "待确认曲线应能被框选"
    );
    assert!(editor.selection_box().is_some(), "待确认曲线应有选中选框");

    // 沿曲线本体按下拖动 (+10 tick, +2 key)：折线命中容差内的点
    press_at(&mut editor, (20.0, 65.0), false);
    assert!(
        editor.editor_state.shape_select.is_dragging(),
        "应能从待确认曲线上起拖"
    );
    move_to(&mut editor, (30.0, 67.0));
    release(&mut editor);

    let path = &editor.editor_state.line_tool.paths[0];
    assert_eq!(
        (path[0].pos, path[1].pos),
        ((10.0, 62.0), (50.0, 72.0)),
        "拖动应平移曲线锚点 (+10,+2)"
    );
    let mirror = &editor.editor_state.shape_select.shapes()[0];
    assert_eq!(
        mirror.source.bounds(),
        Some((10.0, 50.0, 62.0, 72.0)),
        "镜像几何必须跟随锚点平移（tick 10..50 × key 62..72）"
    );
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "拖动待确认曲线不得生成音符"
    );
}

/// 场景三之三：**未 √** 的画刷笔画 → 鼠标工具框选 → 拖动 → 笔画点列被平移
#[test]
fn test_marquee_over_unconfirmed_brush_stroke_drags_points() {
    use lumino_editor_state::BrushStroke;
    let mut editor = editor_with_empty_canvas();
    editor.set_tool(Tool::Brush);
    editor.brush.set_thickness(1);
    editor.editor_state.brush_tool.strokes.push(BrushStroke {
        points: vec![(0.0, 60.0), (20.0, 60.0), (40.0, 60.0)],
        base_track: 1,
    });
    editor.set_tool(Tool::ShapeSelect);

    press_at(&mut editor, (100.0, 90.0), false);
    move_to(&mut editor, (0.0, 40.0));
    release(&mut editor);
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        1,
        "待确认笔画应能被框选"
    );
    assert!(editor.selection_box().is_some(), "待确认笔画应有选中选框");

    press_at(&mut editor, (20.0, 60.0), false);
    assert!(editor.editor_state.shape_select.is_dragging());
    move_to(&mut editor, (30.0, 62.0));
    release(&mut editor);

    assert_eq!(
        editor.editor_state.brush_tool.strokes[0].points,
        vec![(10.0, 62.0), (30.0, 62.0), (50.0, 62.0)],
        "拖动应平移待确认笔画的点列 (+10,+2)"
    );
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "拖动待确认笔画不得生成音符"
    );
}

/// 场景三之四：拖动待确认曲线后 **Ctrl+Z 撤销路径编辑** → 镜像几何必须同帧回灌
///
/// 「撤销待确认路径编辑」（brush/line 的 `*_path` 分支）与工具无关：鼠标工具下按
/// Ctrl+Z 同样会回退待确认几何。镜像若不回灌，就会重现「预览回到原位、选框与描边
/// 留在旧位置 → 框不中 / 框与图案错位」。
#[test]
fn test_undo_pending_path_edit_refreshes_mirror_geometry() {
    let mut editor = editor_with_empty_canvas();
    editor.set_tool(Tool::Curve);
    editor.editor_state.line_tool.paths = vec![vec![
        lumino_editor_state::BezierAnchor::new((0.0, 60.0)),
        lumino_editor_state::BezierAnchor::new((40.0, 70.0)),
    ]];
    editor.editor_state.line_tool.recompute_auto_handles();
    editor.editor_state.line_tool.push_path_history(); // 基准快照
    editor.set_tool(Tool::ShapeSelect);

    press_at(&mut editor, (200.0, 90.0), false);
    move_to(&mut editor, (0.0, 40.0));
    release(&mut editor);
    press_at(&mut editor, (20.0, 65.0), false);
    move_to(&mut editor, (30.0, 67.0));
    release(&mut editor);

    let mirror_id = editor.editor_state.shape_select.shapes()[0].id;
    assert_eq!(
        editor.editor_state.line_tool.paths[0][0].pos,
        (10.0, 62.0),
        "前置：锚点已被拖动平移"
    );

    assert!(editor.undo(), "应能撤销待确认路径编辑");
    assert_eq!(
        editor.editor_state.line_tool.paths[0][0].pos,
        (0.0, 60.0),
        "撤销应回退锚点"
    );
    assert_eq!(
        editor.editor_state.shape_select.shapes()[0].id,
        mirror_id,
        "回灌几何不应换掉镜像 ID（选中态必须保持稳定）"
    );
    assert_eq!(
        editor.editor_state.shape_select.shapes()[0].source.bounds(),
        Some((0.0, 40.0, 60.0, 70.0)),
        "镜像几何必须随撤销同帧回灌（否则选框留在旧位置、框不中）"
    );
    assert_eq!(
        editor.editor_state.shape_select.selected_ids(),
        &[mirror_id],
        "回灌后选中态应保持"
    );
    let rect = editor.selection_box().expect("回灌后仍应有选框");
    for corner in [(0.0, 60.0), (40.0, 70.0)] {
        assert!(
            rect.contains(editor.line_pos_screen_pos(corner)),
            "选框应回到撤销后的几何位置 {corner:?}"
        );
    }
}

/// 场景三之五：删除待确认产物（Delete）→ 几何与镜像一并退场，且**同容器后续镜像下标前移**
#[test]
fn test_delete_pending_shape_removes_geometry_and_keeps_later_mirror_usable() {
    use lumino_editor_state::PendingShapeRef;
    let mut editor = editor_with_empty_canvas();
    // 两个待确认图形：A(0..4 × 60..64)、B(200..204 × 60..64)
    editor.set_tool(Tool::Shape);
    editor.set_shape(lumino_editor_state::ShapeKind::Rectangle);
    editor.editor_state.shape_tool.fill_enabled = false;
    for (t0, t1) in [(0.0f32, 4.0f32), (200.0, 204.0)] {
        editor.handle_shape_tool_pressed(t0, 60.0, false);
        editor.handle_shape_tool_moved(t1, 64.0);
        editor.handle_shape_tool_released();
    }
    editor.set_tool(Tool::ShapeSelect);
    assert_eq!(editor.editor_state.shape_select.len(), 2, "两件待确认图形");

    // 只点选第一件（tick 0..4）
    press_at(&mut editor, (2.0, 62.0), false);
    release(&mut editor);
    assert_eq!(editor.editor_state.shape_select.selection_len(), 1);
    assert!(editor.delete_selected_drawn_shape(), "应删除待确认几何");

    assert_eq!(
        editor.editor_state.shape_tool.shapes.len(),
        1,
        "待确认容器里应只剩一件"
    );
    assert_eq!(
        editor.editor_state.shape_tool.shapes[0].rect,
        (200.0, 60.0, 204.0, 64.0),
        "剩下的应是 B"
    );
    assert_eq!(
        editor.editor_state.shape_select.len(),
        1,
        "镜像应同步退场一件"
    );
    // 剩余镜像的引用必须已前移（删掉的是下标 0，B 现在是下标 0）
    assert_eq!(
        editor.editor_state.shape_select.shapes()[0].pending,
        Some(PendingShapeRef::Shape(0)),
        "同容器后续镜像的下标应前移一位"
    );
    // 仍可框选 + 拖动剩下的那件（引用若错位，这里会拖到不存在的下标）
    press_at(&mut editor, (300.0, 80.0), false);
    move_to(&mut editor, (100.0, 40.0));
    release(&mut editor);
    assert_eq!(editor.editor_state.shape_select.selection_len(), 1);
    press_at(&mut editor, (202.0, 62.0), false);
    assert!(editor.editor_state.shape_select.is_dragging());
    move_to(&mut editor, (212.0, 64.0));
    release(&mut editor);
    assert_eq!(
        editor.editor_state.shape_tool.shapes[0].rect,
        (210.0, 62.0, 214.0, 66.0),
        "剩余待确认图形仍可被拖动"
    );
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "删除 / 拖动待确认产物都不得生成音符"
    );
}

/// 场景四：√ 确认的**多个**图形 → 框选整组 → 并集选框 + 框内空白处整组拖动
#[test]
fn test_marquee_over_two_confirmed_shapes_boxes_union_and_drags_group() {
    let mut editor = editor_with_canvas();
    // 图形 A：0..4 × 60..64；图形 B：200..204 × 60..64（各自 √ 确认）
    for (t0, t1) in [(0.0f32, 4.0f32), (200.0, 204.0)] {
        editor.set_tool(Tool::Shape);
        editor.set_shape(lumino_editor_state::ShapeKind::Rectangle);
        editor.editor_state.shape_tool.fill_enabled = false;
        editor.handle_shape_tool_pressed(t0, 60.0, false);
        editor.handle_shape_tool_moved(t1, 64.0);
        editor.handle_shape_tool_released();
        assert!(editor.confirm_shape_tool(), "矩形应生成音符并登记图形");
    }
    editor.set_tool(Tool::ShapeSelect);

    press_at(&mut editor, (300.0, 100.0), false);
    move_to(&mut editor, (0.0, 40.0));
    release(&mut editor);

    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        2,
        "框内两个图形都应被框选"
    );
    let rect = editor.selection_box().expect("多选应显示并集选框");
    // 两图形之间的空白（tick 100、key 62）必须落在并集选框内 → 可从框内空白整组拖动
    let mid = (100.0, 62.0);
    assert_eq!(
        editor.hit_test_drawn_shape(mid.0, mid.1),
        None,
        "前置：两图形之间是空白"
    );
    assert!(
        rect.contains(editor.line_pos_screen_pos(mid)),
        "并集选框应覆盖两图形之间的空白"
    );
    press_at(&mut editor, mid, false);
    assert!(
        editor.editor_state.shape_select.is_dragging(),
        "框选整组后应能从并集选框内部起拖"
    );
    move_to(&mut editor, (110.0, 64.0));
    release(&mut editor);
    match &editor.editor_state.shape_select.shapes()[0].source {
        lumino_editor_state::DrawnShapeSource::Shape { rect, .. } => {
            assert_eq!(*rect, (10.0, 62.0, 14.0, 66.0), "整组应平移 (+10,+2)")
        }
        other => panic!("期望 Shape 几何，实际 {other:?}"),
    }
}

/// 场景五：**纵向卷帘**下 √ 确认图形 → 框选 → 选框 + 拖动（转置坐标链路）
#[test]
fn test_marquee_in_vertical_roll_shows_box_and_can_drag() {
    let mut editor = editor_with_canvas();
    editor.editor_state.is_vertical_roll = true;
    draw_and_confirm_rect(&mut editor);
    editor.set_tool(Tool::ShapeSelect);

    press_at(&mut editor, (50.0, 80.0), false);
    assert!(
        editor.editor_state.shape_select.is_marqueeing(),
        "纵向卷帘下空白按下也应进入拉框态"
    );
    move_to(&mut editor, (0.0, 55.0));
    release(&mut editor);
    assert_eq!(
        editor.editor_state.shape_select.selection_len(),
        1,
        "纵向卷帘下框内图形应被选中"
    );
    assert!(
        editor.selection_box().is_some(),
        "纵向卷帘下框选后必须显示选中选框"
    );
    press_at(&mut editor, (2.0, 62.0), false);
    assert!(
        editor.editor_state.shape_select.is_dragging(),
        "纵向卷帘下框选后应能起拖"
    );
}

/// 场景六：√ 确认的**画刷笔画** → 框选 → 选框 + 拖动
#[test]
fn test_marquee_over_confirmed_brush_stroke_shows_box_and_can_drag() {
    use lumino_editor_state::BrushStroke;
    let mut editor = editor_with_canvas();
    editor.set_tool(Tool::Brush);
    editor.brush.set_thickness(1);
    editor.editor_state.brush_tool.strokes.push(BrushStroke {
        points: vec![(0.0, 60.0), (20.0, 64.0), (40.0, 68.0)],
        base_track: 1,
    });
    assert!(editor.confirm_brush(), "画刷笔画应生成音符并登记折线");
    editor.set_tool(Tool::ShapeSelect);

    press_at(&mut editor, (100.0, 90.0), false);
    move_to(&mut editor, (0.0, 40.0));
    release(&mut editor);

    assert!(
        editor.editor_state.shape_select.selection_len() > 0,
        "框内笔画应被框选选中"
    );
    assert!(
        editor.selection_box().is_some(),
        "框选笔画后必须显示选中选框"
    );
}
