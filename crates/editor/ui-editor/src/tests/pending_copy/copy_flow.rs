use super::release_copy_drag;
use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;

// ===== BUG 修复验证：无 pending 的 DraggingSelection（批量移动）ghost 增量 =====
// UI 层 `is_ghost_dragging` 判定依赖 `build_ghost_delta_positions` 返回增量位置：
// - 普通批量移动（无 pending/copy）→ 返回全部被拖动音符的 ghost 位置（增量 UpdateMany）
// - 复制模式（pending_copy / DraggingSelectionCopy）→ 返回空（副本实例破坏 GPU 布局，回退全量）

#[test]
fn test_build_ghost_delta_positions_move_without_pending() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 2000.0;
    editor.editor_state.canvas.size_y = 4000.0;
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(600.0, 62, 240.0)],
    );
    editor.editor_state.view.set_snap_precision(10.0);
    editor.selection_insert(0);
    editor.selection_insert(1);

    // 无 Ctrl 在选中框内按下 → DraggingSelection（无任何 pending）
    let origin_x = editor.editor_state.view.tick_to_x(300.0);
    let origin_y = editor.editor_state.view.key_to_y(60) + editor.editor_state.view.zoom_y / 2.0;
    editor.set_ctrl_pressed(false);
    editor.handle_tool_pressed(iced_core::Point::new(origin_x, origin_y), false, 300.0, 60);
    assert!(
        matches!(
            editor.editor_state.interaction.edit_state,
            EditState::DraggingSelection { .. }
        ),
        "应进入 DraggingSelection（移动模式），实际 {:?}",
        editor.editor_state.interaction.edit_state
    );

    // 拖动 +50 tick：300 → 350
    let moved_x = editor.editor_state.view.tick_to_x(350.0);
    editor.handle_moved(iced_core::Point::new(moved_x, origin_y));

    // ghost 增量：两个被拖动音符都应返回 ghost 位置（原件 50，附件 650）
    let visible = vec![0usize, 1];
    let positions = editor.build_ghost_delta_positions(&visible);
    assert_eq!(
        positions.len(),
        2,
        "无 pending 的批量移动应返回全部被拖动音符的增量位置，实际 {:?}",
        positions
    );
    let tick_at = |pos: usize| positions.iter().find(|(p, _)| *p == pos).map(|(_, v)| v.0);
    assert_eq!(tick_at(0), Some(50.0), "原件应 ghost 到 tick 50");
    assert_eq!(tick_at(1), Some(650.0), "附件应 ghost 到 tick 650");
}

#[test]
fn test_build_ghost_delta_positions_disabled_when_copy_pending() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 2000.0;
    editor.editor_state.canvas.size_y = 4000.0;
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.editor_state.view.set_snap_precision(10.0);
    editor.selection_insert(0);

    // pending_copy 存在（拖动中 DraggingSelectionCopy / 提交失败兜底场景）
    // → ghost 增量必须禁用（副本实例破坏 GPU 布局）
    let mut d1 = DragState::from_indices([0], 1, 0, 60);
    d1.set_delta(100, 0);
    editor.pending_copy_drag_state = Some(d1);

    let positions = editor.build_ghost_delta_positions(&[0usize]);
    assert!(
        positions.is_empty(),
        "复制模式应禁用 ghost 增量（回退全量重建），实际 {:?}",
        positions
    );
}

// ===== BUG 修复回归：连续复制「复制下一份」完整交互序列 =====
// - 松手即提交：每次复制副本立即真实化并只保留最新件（副本）框选
// - Ctrl+拖动副本框 = 复制下一份：拖动中旧副本保持 + 新副本跟手（双副本），
//   松手时新副本提交入内存（从副本位置继续偏移）

/// 模拟完整交互序列：复制（松手即提交）→ Ctrl+拖动副本框 → 拖动 → 松手
#[test]
fn test_continuous_copy_from_copy_box_commits_old_and_accumulates() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 2000.0;
    editor.editor_state.canvas.size_y = 4000.0;
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    // 吸附精度设为 10：让测试坐标与吸附网格精确对齐（默认 PPQ=1920 会吞掉小位移）
    editor.editor_state.view.set_snap_precision(10.0);
    editor.selection_insert(0);

    // 第一次复制：delta=(100, 0) → 松手即提交，副本真实化（tick 100）
    let mut d1 = DragState::from_indices([0], 1, 0, 60);
    d1.set_delta(100, 0);
    release_copy_drag(&mut editor, d1);
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        2,
        "第一次复制松手后副本应已写入内存"
    );
    assert!(editor.pending_copy_drag_state.is_none());

    // 保持 Ctrl，在副本框位置（tick 100 中心，音符长度 480 → 中心 340）再次按下
    // 预计算所有屏幕坐标（避免借用跨越可变调用）
    let copy_center_y =
        editor.editor_state.view.key_to_y(60) + editor.editor_state.view.zoom_y / 2.0;
    let copy_center_x = editor.editor_state.view.tick_to_x(340.0);
    let moved_x = editor.editor_state.view.tick_to_x(390.0);
    let copy2_center_x = editor.editor_state.view.tick_to_x(390.0);
    let moved_x_alt = editor.editor_state.view.tick_to_x(440.0);
    editor.set_ctrl_pressed(true);
    editor.handle_tool_pressed(
        iced_core::Point::new(copy_center_x, copy_center_y),
        false,
        340.0,
        60,
    );

    match editor.editor_state.interaction.edit_state {
        EditState::DraggingSelectionCopy { .. } => {}
        other => panic!(
            "Ctrl+拖动副本框应进入 DraggingSelectionCopy（复制下一份），实际 {:?}",
            other
        ),
    }
    // 拖动中 document 未写入（ghost 方案：松手才提交）
    assert_eq!(editor.editor_state.data.current_track_note_count(), 2);

    // 拖动 +50 tick：340 → 390，副本2 从副本1 位置继续偏移
    editor.handle_moved(iced_core::Point::new(moved_x, copy_center_y));

    // 渲染验证：原件(0) + 旧副本(100) + 新副本(150) 三份并存
    let mut visible: Vec<(f32, u16, f32)> = Vec::new();
    editor.collect_visible_note_data(&mut visible, None, 0.0);
    let mut ticks: Vec<f32> = visible.iter().map(|(t, _, _)| *t).collect();
    ticks.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(
        ticks,
        vec![0.0, 100.0, 150.0],
        "拖动中应显示 原件(0) + 旧副本(100) + 新副本(150)，实际 {:?}",
        ticks
    );

    // 松手：新副本提交入内存（count=3），选中副本2（tick 150）
    editor.handle_released();
    assert!(editor.pending_copy_drag_state.is_none());
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        3,
        "松手即提交：内存 = 原件 + 副本1 + 副本2"
    );
    // 只保留最新件框选：选择框应位于副本2（tick 150）位置，原件不再框选
    let rects = editor.get_selection_box_rects();
    assert_eq!(rects.len(), 1, "复制模式只显示一个副本框（最新件框选）");
    let v = &editor.editor_state.view;
    let (r_x1, r_x2, _, _) = rects[0];
    assert!(
        (r_x1 - v.tick_to_x(150.0)).abs() < 0.001 && (r_x2 - v.tick_to_x(630.0)).abs() < 0.001,
        "选择框应覆盖最新副本 [150, 630]，实际 [{}, {}]",
        r_x1,
        r_x2
    );
    // 原件位置不再命中任何框（原件不保留框选）；
    // 原件独有区域 tick 50（原件 [0,480]，副本 [150,630]，重叠区 150-480）
    assert_eq!(
        editor.hit_test_selection_box(iced_core::Point::new(
            v.tick_to_x(50.0),
            v.key_to_y(60) + v.zoom_y / 2.0
        )),
        None,
        "原件独有区域不应命中选择框（原件不再框选）"
    );

    // 连续第三次：Ctrl+拖动副本2 框（tick 150 中心 → 390）→ 从副本2 继续复制
    editor.handle_tool_pressed(
        iced_core::Point::new(copy2_center_x, copy_center_y),
        false,
        390.0,
        60,
    );
    assert!(
        matches!(
            editor.editor_state.interaction.edit_state,
            EditState::DraggingSelectionCopy { .. }
        ),
        "Ctrl+拖动副本2 框应继续复制下一份，实际 {:?}",
        editor.editor_state.interaction.edit_state
    );
    editor.handle_moved(iced_core::Point::new(moved_x_alt, copy_center_y));
    editor.handle_released();
    assert!(editor.pending_copy_drag_state.is_none());
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        4,
        "第三次复制：内存 = 原件 + 副本1 + 副本2 + 副本3"
    );
}
