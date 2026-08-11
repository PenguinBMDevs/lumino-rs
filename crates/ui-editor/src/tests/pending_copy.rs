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
    // 副本被选中（按参数精确匹配，不误选原始音符）
    assert!(editor.editor_state.interaction.selected_notes.contains(&1));
    assert!(!editor.editor_state.interaction.selected_notes.contains(&0));
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
    // 副本（散布索引 1/3/5）全部选中，原始音符（0/2/4）不选中
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
            "原始音符索引 {} 不应被选中",
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

// ===== 复制后选择框覆盖原件∪副本（新框选覆盖复制对象） =====

#[test]
fn test_selection_box_covers_origin_and_copy() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.selection_insert(0);

    // 复制松手：pending_copy delta=100 → 副本位于 tick [100, 580]
    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(100, 0);
    editor.pending_copy_drag_state = Some(drag);

    // 选择框必须覆盖原件 [0, 480] ∪ 副本 [100, 580]
    let (x1, x2, _, _) = editor.get_selection_box_bounds().expect("应有选择框");
    let v = &editor.editor_state.view;
    let origin_x = v.tick_to_x(0.0);
    let origin_end_x = v.tick_to_x(480.0);
    let copy_x = v.tick_to_x(100.0);
    let copy_end_x = v.tick_to_x(580.0);
    assert!(
        (x1 - origin_x).abs() < 0.001,
        "选择框左边界应覆盖原件起点（实际 x1={}, 期望 {}）",
        x1,
        origin_x
    );
    assert!(
        (x2 - copy_end_x).abs() < 0.001,
        "选择框右边界应覆盖副本终点（实际 x2={}, 期望 {}）",
        x2,
        copy_end_x
    );
    assert!(
        x2 > origin_end_x,
        "选择框应超出原件右边界以覆盖副本（x2={}, origin_end={}）",
        x2,
        origin_end_x
    );
    assert!(
        x1 <= copy_x,
        "选择框应覆盖副本起点（x1={}, copy_x={}）",
        x1,
        copy_x
    );
}

#[test]
fn test_selection_box_hit_test_inside_copy_area() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.selection_insert(0);

    // 复制松手：副本在 tick 100（delta=100）
    let mut drag = DragState::from_indices([0], 1, 0, 60);
    drag.set_delta(100, 0);
    editor.pending_copy_drag_state = Some(drag);

    // 点击副本中心位置（tick 340, key 60）→ 应命中选择框 Inside
    let v = &editor.editor_state.view;
    let copy_center_x = v.tick_to_x(340.0);
    let copy_center_y = v.key_to_y(60) + v.zoom_y / 2.0;
    let hit = editor.hit_test_selection_box(iced_core::Point::new(copy_center_x, copy_center_y));
    assert_eq!(
        hit,
        Some(crate::SelectionHitType::Inside),
        "副本位置应命中选择框内部（新框选覆盖复制对象）"
    );

    // 从副本位置 Ctrl+拖动 → 进入复制拖拽（新件拥有复制能力）
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
            "从副本位置 Ctrl+拖动应进入 DraggingSelectionCopy，实际 {:?}",
            other
        ),
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
