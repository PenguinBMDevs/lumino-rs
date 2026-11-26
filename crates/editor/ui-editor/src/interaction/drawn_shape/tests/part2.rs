//! 由 `tests.rs` 拆出的后半段测试，按原顺序保留。

use super::*;

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
