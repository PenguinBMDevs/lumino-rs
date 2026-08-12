//! Ctrl+拖动复制音符测试：pending_copy_drag_state + 延迟提交
//!
//! 覆盖：
//! - `handle_pointer_pressed` Ctrl + 选择框内部 → `DraggingSelectionCopy`
//! - `handle_released` 松手后保存到 `pending_copy_drag_state`（不立即写入）
//! - `commit_pending_copy`：batch_insert 写入内存层并选中新副本
//! - 原始音符保持不变（复制不移动）
//! - `is_editing` / `has_pending_drag` / `commit_current_edit` 状态判定
//!
//! 2026-08 单一权威源：测试种子经 `test_helpers::seed_notes` 写入 document。

use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;

/// 模拟 released.rs 中复制拖动松手：直接进入 DraggingSelectionCopy 再 handle_released
fn release_copy_drag(editor: &mut Editor, drag_state: DragState) {
    editor.editor_state.interaction.edit_state = EditState::DraggingSelectionCopy {
        drag_state: drag_state.clone(),
    };
    editor.handle_released();
}

// ===== 初始状态判定 =====

#[test]
fn test_pending_copy_state_initial_is_none() {
    let editor = Editor::new();
    assert!(editor.pending_copy_drag_state.is_none());
    assert!(!editor.has_pending_drag());
    assert!(!editor.is_editing());
}

// ===== commit_pending_copy 行为 =====

#[test]
fn test_commit_pending_copy_when_none_returns_false() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    assert!(!editor.commit_pending_copy());
    // 音符数不变
    assert_eq!(editor.editor_state.data.current_track_note_count(), 1);
}

#[test]
fn test_commit_pending_copy_zero_delta_returns_false_and_clears() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    // delta 为零的 pending 复制（未拖动，直接松手）
    let zero_drag = DragState::from_single(0, 1, 0, 60);
    editor.pending_copy_drag_state = Some(zero_drag);

    assert!(
        !editor.commit_pending_copy(),
        "delta 零应返回 false（未产生副本）"
    );
    assert!(editor.pending_copy_drag_state.is_none(), "pending 应被清空");
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        1,
        "不应插入任何音符"
    );
}

#[test]
fn test_commit_pending_copy_inserts_copies_and_selects_them() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(240.0, 62, 240.0)],
    );

    // 选中索引 0，delta=(200, 7)：复制出一个偏移副本
    let mut drag = DragState::from_indices([0], 2, 0, 60);
    drag.set_delta(200, 7);
    editor.pending_copy_drag_state = Some(drag);

    assert!(editor.commit_pending_copy());
    let data = &editor.editor_state.data;
    // 原始 2 个音符保留，副本插入：2 → 3 个音符
    assert_eq!(data.current_track_note_count(), 3);
    // batch_insert 按 start_tick 有序插入：副本 tick=200 落在 0 与 240 之间 → 索引 1
    assert_eq!(data.get_note_view(0).expect("note should exist").tick, 0.0);
    assert_eq!(
        data.get_note_view(0).expect("note should exist").key,
        60,
        "原始音符不变"
    );
    assert_eq!(
        data.get_note_view(1).expect("note should exist").tick,
        200.0
    );
    assert_eq!(data.get_note_view(1).expect("note should exist").key, 67);
    assert_eq!(
        data.get_note_view(1).expect("note should exist").length,
        480.0,
        "长度复制自原始"
    );
    assert_eq!(
        data.get_note_view(2).expect("note should exist").tick,
        240.0
    );
    assert_eq!(
        data.get_note_view(2).expect("note should exist").key,
        62,
        "未选中音符不变"
    );
    // 副本被选中（按参数精确匹配，不误选未选中的原始音符）
    assert!(editor.editor_state.interaction.selected_notes.contains(&1));
    assert!(
        !editor.editor_state.interaction.selected_notes.contains(&0),
        "原件不应保持选中（只保留最新件框选，用户要求）"
    );
    assert!(!editor.editor_state.interaction.selected_notes.contains(&2));
    // pending 已清空
    assert!(editor.pending_copy_drag_state.is_none());
    assert!(!editor.has_pending_drag());
}

#[test]
fn test_commit_pending_copy_multiple_notes() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 480.0),
            Note::new(240.0, 62, 240.0),
            Note::new(480.0, 64, 120.0),
        ],
    );

    // 全选复制：delta=(100, 3)
    let mut drag = DragState::from_indices([0, 1, 2], 3, 0, 60);
    drag.set_delta(100, 3);
    editor.pending_copy_drag_state = Some(drag);

    assert!(editor.commit_pending_copy());
    let data = &editor.editor_state.data;
    assert_eq!(data.current_track_note_count(), 6);
    // 有序插入后交错排列：原始 0/240/480 与副本 100/340/580
    // 0: 原始0    1: 副本100   2: 原始240   3: 副本340   4: 原始480   5: 副本580
    assert_eq!(data.get_note_view(0).expect("note should exist").tick, 0.0);
    assert_eq!(
        data.get_note_view(1).expect("note should exist").tick,
        100.0
    );
    assert_eq!(data.get_note_view(1).expect("note should exist").key, 63);
    assert_eq!(
        data.get_note_view(2).expect("note should exist").tick,
        240.0
    );
    assert_eq!(
        data.get_note_view(3).expect("note should exist").tick,
        340.0
    );
    assert_eq!(data.get_note_view(3).expect("note should exist").key, 65);
    assert_eq!(
        data.get_note_view(4).expect("note should exist").tick,
        480.0
    );
    assert_eq!(
        data.get_note_view(5).expect("note should exist").tick,
        580.0
    );
    assert_eq!(data.get_note_view(5).expect("note should exist").key, 67);
    // 副本（散布索引 1/3/5）选中；原件（0/2/4）不再框选（只保留最新件框选）
    for i in [1usize, 3, 5] {
        assert!(
            editor.editor_state.interaction.selected_notes.contains(&i),
            "副本索引 {} 应被选中",
            i
        );
    }
    for i in [0usize, 2, 4] {
        assert!(
            !editor.editor_state.interaction.selected_notes.contains(&i),
            "原件索引 {} 不应保持选中（只保留最新件框选）",
            i
        );
    }
}

#[test]
fn test_commit_pending_copy_clamps_key_range() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // key 下移 100：60 - 100 = -40 → clamp 到 0
    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(0, -100);
    editor.pending_copy_drag_state = Some(drag);

    assert!(editor.commit_pending_copy());
    let data = &editor.editor_state.data;
    assert_eq!(data.current_track_note_count(), 2);
    assert_eq!(
        data.get_note_view(0).expect("note should exist").key,
        60,
        "原始音符不变"
    );
    assert_eq!(
        data.get_note_view(1).expect("note should exist").key,
        0,
        "副本 key 应 clamp 到 0"
    );
}

// ===== handle_released 松手行为 =====

#[test]
fn test_released_copy_drag_saves_pending_copy() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(240.0, 62, 240.0)],
    );
    // 预选中索引 0（进入 DraggingSelectionCopy 的前提：已有选区）
    editor.selection_insert(0);

    // 复制拖动中（Ctrl+拖动）：原始音符不动，仅维护 delta
    let mut drag = DragState::from_indices([0], 2, 0, 60);
    drag.set_delta(200, 7);
    release_copy_drag(&mut editor, drag);

    // 松手后：不写入 document，保存到 pending_copy_drag_state
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        2,
        "document 未变"
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("note should exist")
            .tick,
        0.0
    );
    let pending = editor
        .pending_copy_drag_state
        .as_ref()
        .expect("pending_copy 应已保存");
    assert_eq!(pending.delta_tick, 200);
    assert_eq!(pending.delta_key, 7);
    // 选中集合保留（pending 状态下仍显示框选）
    assert!(editor.has_selection());
    // 编辑状态回到 Idle
    assert_eq!(editor.editor_state.interaction.edit_state, EditState::Idle);
}

#[test]
fn test_released_copy_drag_zero_delta_does_not_save() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // 未拖动（delta 零）直接松手 → 不产生 pending
    let zero_drag = DragState::from_single(0, 1, 0, 60);
    release_copy_drag(&mut editor, zero_drag);

    assert!(editor.pending_copy_drag_state.is_none());
    assert_eq!(editor.editor_state.data.current_track_note_count(), 1);
}

#[test]
fn test_released_copy_drag_keeps_selection() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(240.0, 62, 240.0)],
    );
    // 预选中索引 0、1
    editor.selection_insert(0);
    editor.selection_insert(1);

    let mut drag = DragState::from_indices([0, 1], 2, 0, 60);
    drag.set_delta(50, 0);
    release_copy_drag(&mut editor, drag);

    assert!(editor.pending_copy_drag_state.is_some());
    assert!(
        editor.has_selection(),
        "pending 状态下选区应保留（副本仍显示在 UI 层）"
    );
}

// ===== handle_pointer_pressed Ctrl + 选择框内部 → 复制拖拽 =====

/// 计算选中音符选择框的屏幕中心点（保证 Inside 命中）
fn selection_box_center(editor: &Editor) -> iced_core::Point {
    let (x1, x2, y1, y2) = editor
        .get_selection_box_bounds()
        .expect("有选中音符时应能计算选择框");
    iced_core::Point::new((x1 + x2) / 2.0, (y1 + y2) / 2.0)
}

#[test]
fn test_pointer_pressed_ctrl_inside_enters_copy_state() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.selection_insert(0);
    editor.set_ctrl_pressed(true);

    // 点击选择框内部 → Ctrl 按下 → DraggingSelectionCopy
    let center = selection_box_center(&editor);
    editor.handle_tool_pressed(center, false, 0.0, 60);

    match editor.editor_state.interaction.edit_state {
        EditState::DraggingSelectionCopy { .. } => {}
        other => panic!("Ctrl 拖动应进入 DraggingSelectionCopy，实际 {:?}", other),
    }
}

#[test]
fn test_pointer_pressed_without_ctrl_enters_move_state() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.selection_insert(0);
    editor.set_ctrl_pressed(false);

    // 点击选择框内部 → 未按 Ctrl → DraggingSelection（移动）
    let center = selection_box_center(&editor);
    editor.handle_tool_pressed(center, false, 0.0, 60);

    match editor.editor_state.interaction.edit_state {
        EditState::DraggingSelection { .. } => {}
        other => panic!("非 Ctrl 拖动应进入 DraggingSelection，实际 {:?}", other),
    }
}

// ===== 状态判定 / 自动提交 =====

#[test]
fn test_is_editing_with_pending_copy_returns_true() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.pending_copy_drag_state = Some(DragState::from_single(0, 1, 0, 60));
    assert!(editor.is_editing(), "pending copy 应视为编辑状态");
    assert!(editor.has_pending_drag());
}

#[test]
fn test_commit_current_edit_commits_pending_copy() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // 模拟：复制拖动松手后（pending_copy 存在），用户直接触发 Save/Play/Export
    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(100, 0);
    editor.pending_copy_drag_state = Some(drag);

    assert!(editor.commit_current_edit());
    // 副本已写入内存层
    assert_eq!(editor.editor_state.data.current_track_note_count(), 2);
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(1)
            .expect("note should exist")
            .tick,
        100.0
    );
    assert!(editor.pending_copy_drag_state.is_none());
}

#[test]
fn test_flush_pending_drag_commits_copy_on_empty_click() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // 复制松手后 pending_copy 存在
    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(100, 0);
    editor.pending_copy_drag_state = Some(drag);

    // 模拟点击空白处（flush_pending_drag 由 handle_pointer_pressed 空白分支调用）
    // 直接调用私有路径不可达，改用公开入口：点击空白会先 flush 再开始新框选
    editor.handle_tool_pressed(iced_core::Point::new(9999.0, 9999.0), false, 9999.0, 0);

    // 副本已写入内存层（点击空白处退出框选状态 → 写入）
    assert_eq!(editor.editor_state.data.current_track_note_count(), 2);
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(1)
            .expect("note should exist")
            .tick,
        100.0
    );
    assert!(
        editor.pending_copy_drag_state.is_none(),
        "复制提交后 pending 应清空"
    );
}

// ===== 移动 + 复制并存（提交顺序正确性） =====

#[test]
fn test_move_and_copy_both_pending_commit_in_correct_order() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(240.0, 62, 240.0)],
    );

    // 场景：先移动选中音符（松手 → pending_drag），再 Ctrl 复制同一选区（松手 → pending_copy）
    // 移动音符 A（索引 0）→ delta=(100, 0)
    let mut move_drag = DragState::from_indices([0], 2, 0, 60);
    move_drag.set_delta(100, 0);
    editor.pending_drag_state = Some(move_drag);
    // 复制音符 B（索引 1）→ delta=(50, 0)：副本 tick=290 追加在尾部
    let mut copy_drag = DragState::from_indices([1], 2, 0, 60);
    copy_drag.set_delta(50, 0);
    editor.pending_copy_drag_state = Some(copy_drag);

    // 点击空白处：flush → 先 drain 移动异步提交，再提交复制
    editor.handle_tool_pressed(iced_core::Point::new(9999.0, 9999.0), false, 9999.0, 0);

    let data = &editor.editor_state.data;
    // 音符 A 已移动（tick 0 → 100），B 保持（240），副本 B'（290）已写入
    assert_eq!(data.current_track_note_count(), 3);
    // 排序后: [A(100), B(240), B'(290)]
    assert_eq!(
        data.get_note_view(0).expect("note should exist").tick,
        100.0,
        "A 应被移动"
    );
    assert_eq!(data.get_note_view(0).expect("note should exist").key, 60);
    assert_eq!(
        data.get_note_view(1).expect("note should exist").tick,
        240.0,
        "B 不变"
    );
    assert_eq!(
        data.get_note_view(2).expect("note should exist").tick,
        290.0,
        "B 副本已写入"
    );
    assert_eq!(data.get_note_view(2).expect("note should exist").key, 62);
    assert!(editor.pending_drag_state.is_none());
    assert!(editor.pending_copy_drag_state.is_none());
    assert!(!editor.has_pending_drag());
}

#[test]
fn test_commit_current_edit_with_move_and_copy_keeps_both() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[Note::new(0.0, 60, 480.0), Note::new(240.0, 62, 240.0)],
    );

    // 移动 A（索引 0）delta=(100,0) + 复制 B（索引 1）delta=(50,0)
    let mut move_drag = DragState::from_indices([0], 2, 0, 60);
    move_drag.set_delta(100, 0);
    editor.pending_drag_state = Some(move_drag);
    let mut copy_drag = DragState::from_indices([1], 2, 0, 60);
    copy_drag.set_delta(50, 0);
    editor.pending_copy_drag_state = Some(copy_drag);

    // Save/Play/Export 前的自动提交：drain 移动后写副本
    assert!(editor.commit_current_edit());

    let data = &editor.editor_state.data;
    assert_eq!(data.current_track_note_count(), 3);
    assert_eq!(
        data.get_note_view(0).expect("note should exist").tick,
        100.0,
        "A 应被移动"
    );
    assert_eq!(
        data.get_note_view(2).expect("note should exist").tick,
        290.0,
        "B 副本已写入"
    );
    assert!(editor.pending_drag_state.is_none());
    assert!(editor.pending_copy_drag_state.is_none());
}

// ===== 复制后只保留最新件（副本）框选（原件不再框选，用户要求） =====

#[test]
fn test_selection_box_rects_only_copy_box() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.selection_insert(0);

    // 复制松手：delta=600 → 副本位于 tick [600, 1080]，与原件 [0, 480] 完全分离
    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(600, 0);
    editor.pending_copy_drag_state = Some(drag);

    // 单框 = 副本框 [600, 1080]（最新件框选；原件不再框选）
    let rects = editor.get_selection_box_rects();
    assert_eq!(rects.len(), 1, "复制模式应只返回一个副本框（最新件框选）");
    let v = &editor.editor_state.view;
    let copy_x = v.tick_to_x(600.0);
    let copy_end_x = v.tick_to_x(1080.0);
    let (c_x1, c_x2, _, _) = rects[0];
    assert!(
        (c_x1 - copy_x).abs() < 0.001,
        "副本框左边界 = 副本起点（实际 {}, 期望 {}）",
        c_x1,
        copy_x
    );
    assert!(
        (c_x2 - copy_end_x).abs() < 0.001,
        "副本框右边界 = 副本终点（实际 {}, 期望 {}）",
        c_x2,
        copy_end_x
    );
    // 兼容入口返回并集（此时 = 副本框本身）
    let (u_x1, u_x2, _, _) = editor.get_selection_box_bounds().expect("应有并集框");
    assert!(
        (u_x1 - copy_x).abs() < 0.001 && (u_x2 - copy_end_x).abs() < 0.001,
        "兼容入口 bounds 应为副本框 [{}, {}]，实际 [{}, {}]",
        copy_x,
        copy_end_x,
        u_x1,
        u_x2
    );
}

#[test]
fn test_selection_box_hit_test_copy_box_only() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.selection_insert(0);

    // 复制松手：delta=600 → 副本位于 tick [600, 1080]，与原件 [0, 480] 完全分离
    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(600, 0);
    editor.pending_copy_drag_state = Some(drag);

    let v = &editor.editor_state.view;
    let copy_center_y = v.key_to_y(60) + v.zoom_y / 2.0;
    // 副本中心（tick 840）→ Inside（副本框命中 = 最新件框选）
    let copy_center_x = v.tick_to_x(840.0);
    assert_eq!(
        editor.hit_test_selection_box(iced_core::Point::new(copy_center_x, copy_center_y)),
        Some(crate::SelectionHitType::Inside),
        "副本位置应命中副本框内部"
    );
    // 原件中心（tick 240）→ 不命中任何框（原件不再框选，用户要求）
    let origin_center_x = v.tick_to_x(240.0);
    assert_eq!(
        editor.hit_test_selection_box(iced_core::Point::new(origin_center_x, copy_center_y)),
        None,
        "原件位置不应命中任何选择框（原件不保留框选）"
    );

    // 副本框内 Ctrl+拖动 → 复制下一份（DraggingSelectionCopy）
    editor.set_ctrl_pressed(true);
    editor.handle_tool_pressed(
        iced_core::Point::new(copy_center_x, copy_center_y),
        false,
        840.0,
        60,
    );
    match editor.editor_state.interaction.edit_state {
        EditState::DraggingSelectionCopy { .. } => {}
        other => panic!(
            "Ctrl+拖动副本框应进入 DraggingSelectionCopy（复制下一份），实际 {:?}",
            other
        ),
    }
    // 内存未写入：pending 保留、document 未变（先 UI 后内存）
    assert!(
        editor.pending_copy_drag_state.is_some(),
        "再次 Ctrl 拖动不应提交 pending 复制"
    );
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        1,
        "document 不应被写入"
    );

    // 原件位置（无框）Ctrl+按下 → priority 2 单音符命中 → 单音符编辑（移动原件）
    editor.handle_tool_pressed(
        iced_core::Point::new(origin_center_x, copy_center_y),
        false,
        240.0,
        60,
    );
    match editor.editor_state.interaction.edit_state {
        EditState::Dragging { .. } | EditState::PendingDrag { .. } => {}
        other => panic!("原件位置应进入单音符编辑（移动原件），实际 {:?}", other),
    }
}

// ===== pending_copy 累积模式（新件/原件均可再次复制） =====

#[test]
fn test_released_copy_drag_accumulates_delta() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.selection_insert(0);

    // 第一次复制：副本偏移 100
    let mut d1 = DragState::from_indices([0], 1, 0, 60);
    d1.set_delta(100, 0);
    release_copy_drag(&mut editor, d1);
    assert_eq!(
        editor
            .pending_copy_drag_state
            .as_ref()
            .expect("note should exist")
            .delta_tick,
        100
    );

    // 第二次复制：从副本位置（tick 100）继续拖动 50 → 副本偏移 150
    let mut d2 = DragState::from_indices([0], 1, 100, 60);
    d2.set_delta(50, 0);
    release_copy_drag(&mut editor, d2);
    let pending = editor
        .pending_copy_drag_state
        .as_ref()
        .expect("note should exist");
    assert_eq!(
        pending.delta_tick, 150,
        "累积模式：副本从上次副本位置继续偏移"
    );
    assert_eq!(pending.delta_key, 0);
}

#[test]
fn test_released_copy_drag_accumulates_delta_across_axes() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.selection_insert(0);

    // 第一次复制：tick+100, key+5
    let mut d1 = DragState::from_indices([0], 1, 0, 60);
    d1.set_delta(100, 5);
    release_copy_drag(&mut editor, d1);

    // 第二次复制：从副本位置再拖 tick-30, key+2
    let mut d2 = DragState::from_indices([0], 1, 100, 65);
    d2.set_delta(-30, 2);
    release_copy_drag(&mut editor, d2);
    let pending = editor
        .pending_copy_drag_state
        .as_ref()
        .expect("note should exist");
    assert_eq!(pending.delta_tick, 70, "累积 tick: 100 + (-30)");
    assert_eq!(pending.delta_key, 7, "累积 key: 5 + 2");
}

// ===== BUG 复现：pending_copy 存在时，无 Ctrl 在副本框内拖动（移动模式） =====
// 期望：原件跟随鼠标移动（drag delta 生效），副本跟随原件同步平移
//（原件不再框选——用户要求；移动入口 = 副本框）

#[test]
fn test_move_drag_from_original_with_pending_copy_moves_both() {
    let mut editor = Editor::new();
    // 设置视口尺寸（测试环境无真实窗口，默认 size_x=0 导致可见 tick 范围为 0）
    editor.editor_state.canvas.size_x = 2000.0;
    editor.editor_state.canvas.size_y = 4000.0;
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.editor_state.view.set_snap_precision(10.0);
    editor.selection_insert(0);

    // 先 Ctrl+复制：delta=(100, 0)，副本位于 tick 100（UI 层，未提交）
    let mut d1 = DragState::from_indices([0], 1, 0, 60);
    d1.set_delta(100, 0);
    release_copy_drag(&mut editor, d1);

    // 无 Ctrl，在副本框内（tick 340 中心）按下 → DraggingSelection（移动模式）
    let copy_x = editor.editor_state.view.tick_to_x(340.0);
    let copy_y = editor.editor_state.view.key_to_y(60) + editor.editor_state.view.zoom_y / 2.0;
    let copy_center = iced_core::Point::new(copy_x, copy_y);
    editor.set_ctrl_pressed(false);
    editor.handle_tool_pressed(copy_center, false, 340.0, 60);

    match &editor.editor_state.interaction.edit_state {
        EditState::DraggingSelection { drag_state } => {
            assert_eq!(drag_state.delta_tick, 0, "按下时 delta 应为 0");
            assert!(
                !drag_state.selected.is_empty() && drag_state.selected[0],
                "原件索引应被选中参与拖动"
            );
        }
        other => panic!("应进入 DraggingSelection（移动），实际 {:?}", other),
    }

    // 拖动 +50 tick：340 → 390
    let moved_x = editor.editor_state.view.tick_to_x(390.0);
    editor.handle_moved(iced_core::Point::new(moved_x, copy_y));

    if let EditState::DraggingSelection { drag_state } = &editor.editor_state.interaction.edit_state
    {
        assert_eq!(drag_state.delta_tick, 50, "拖动后 drag_state.delta 应为 50");
    }

    // 渲染层验证：原件 ghost 应在 tick 50，副本 ghost 应在 tick 150
    let mut visible: Vec<(f32, u16, f32)> = Vec::new();
    editor.collect_visible_note_data(&mut visible, None, 0.0);
    let mut ticks: Vec<f32> = visible.iter().map(|(t, _, _)| *t).collect();
    ticks.sort_by(|a, b| a.total_cmp(b));
    assert_eq!(
        ticks,
        vec![50.0, 150.0],
        "原件(50)与副本(150)都应跟随拖动偏移，实际 {:?}",
        ticks
    );
}

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

    // pending_copy 存在（复制未提交）→ ghost 增量必须禁用（副本实例破坏 GPU 布局）
    let mut d1 = DragState::from_indices([0], 1, 0, 60);
    d1.set_delta(100, 0);
    release_copy_drag(&mut editor, d1);

    let positions = editor.build_ghost_delta_positions(&[0usize]);
    assert!(
        positions.is_empty(),
        "复制模式应禁用 ghost 增量（回退全量重建），实际 {:?}",
        positions
    );
}

// ===== BUG 修复回归：连续复制「复制下一份」完整交互序列 =====
// - 复制后只保留最新件（副本）框选，原件不再框选（用户要求）
// - Ctrl+拖动副本框 = 复制下一份：拖动中旧副本保持 + 新副本跟手（双副本），
//   松手时旧副本提交入内存、新副本累积（BUG 2 修复，不再被吞并）

/// 模拟完整交互序列：复制松手（pending）→ Ctrl+拖动副本框 → 拖动 → 松手
#[test]
fn test_continuous_copy_from_copy_box_commits_old_and_accumulates() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 2000.0;
    editor.editor_state.canvas.size_y = 4000.0;
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    // 吸附精度设为 10：让测试坐标与吸附网格精确对齐（默认 PPQ=1920 会吞掉小位移）
    editor.editor_state.view.set_snap_precision(10.0);
    editor.selection_insert(0);

    // 第一次复制：delta=(100, 0) → 副本位于 tick 100（UI 层，未写内存）
    let mut d1 = DragState::from_indices([0], 1, 0, 60);
    d1.set_delta(100, 0);
    release_copy_drag(&mut editor, d1);
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        1,
        "第一次复制松手后 document 不应变"
    );

    // 保持 Ctrl，在副本框位置（tick 100 中心，音符长度 480 → 中心 340）再次按下
    // 预计算所有屏幕坐标（避免借用跨越可变调用）
    let copy_center_y =
        editor.editor_state.view.key_to_y(60) + editor.editor_state.view.zoom_y / 2.0;
    let copy_center_x = editor.editor_state.view.tick_to_x(340.0);
    let moved_x = editor.editor_state.view.tick_to_x(390.0);
    let copy2_center_x = editor.editor_state.view.tick_to_x(390.0);
    let moved_x_2 = editor.editor_state.view.tick_to_x(440.0);
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
    // 按下不提交：document 未写入（先 UI 后内存）
    assert_eq!(editor.editor_state.data.current_track_note_count(), 1);

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

    // 松手：旧副本提交入内存（count=2），新副本累积 delta=150
    editor.handle_released();
    let pending = editor
        .pending_copy_drag_state
        .as_ref()
        .expect("累积后 pending 应存在");
    assert_eq!(
        pending.delta_tick, 150,
        "累积模式：100 + 50 = 150（新副本相对原件的总偏移）"
    );
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        2,
        "旧副本应已提交入内存（真实化）"
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
    editor.handle_moved(iced_core::Point::new(moved_x_2, copy_center_y));
    editor.handle_released();
    let pending = editor
        .pending_copy_drag_state
        .as_ref()
        .expect("note should exist");
    assert_eq!(pending.delta_tick, 200, "连续复制：150 + 50 = 200");
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        3,
        "第二次连续复制：副本2 提交入内存，内存 = 原件 + 副本1 + 副本2"
    );
}
