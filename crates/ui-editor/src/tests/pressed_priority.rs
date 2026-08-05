//! `handle_pointer_pressed` 命中优先级测试
//!
//! 验证修复后的命中优先级：
//! 1. 有选中音符时，`hit_test_selection_box` 优先于 `hit_test_note`
//! 2. 选择框内部 → `DraggingSelection`（即使同时命中音符边缘）
//! 3. 选择框边缘 → `ResizingSelectionStart/End`（即使同时命中音符）
//! 4. 选择框外命中音符 → 单音符编辑（切换选区）
//! 5. 无选中音符 + 命中音符 → 单音符编辑
//!
//! **修复背景**：原实现 `hit_test_note` 优先，框选框内点击若命中选中音符边缘，
//! 会误进入 `ResizingStart/End` 单音符拉伸，导致框选拖动无法触发。

use crate::EditState;
use crate::Editor;
use crate::HitType;
use crate::note::Note;
use crate::tests::test_helpers;
use iced_core::Point;

/// 在选择框中心位置返回 Point（确保 Inside 命中）
///
/// 基于 `get_selection_box_bounds` 计算，自动反映 ghost 偏移后的视觉边界。
fn pos_inside_selection(editor: &Editor, _note_idx: usize) -> Point {
    let (x_min, x_max, y_min, y_max) = editor
        .get_selection_box_bounds()
        .expect("选中音符应存在选择框边界");
    Point::new((x_min + x_max) / 2.0, (y_min + y_max) / 2.0)
}

/// 在选择框左边缘返回 Point
fn pos_at_left_edge(editor: &Editor, _note_idx: usize) -> Point {
    let (x_min, _x_max, y_min, y_max) = editor
        .get_selection_box_bounds()
        .expect("选中音符应存在选择框边界");
    Point::new(x_min, (y_min + y_max) / 2.0)
}

/// 在选择框右边缘返回 Point
fn pos_at_right_edge(editor: &Editor, _note_idx: usize) -> Point {
    let (_x_min, x_max, y_min, y_max) = editor
        .get_selection_box_bounds()
        .expect("选中音符应存在选择框边界");
    Point::new(x_max, (y_min + y_max) / 2.0)
}

/// 选择框外，指定音符上的 Point
///
/// 2026-08 单一权威源：经 get_note_view 读取（NoteView: tick f32/key u16）。
fn pos_on_note_outside_selection(editor: &Editor, note_idx: usize) -> Point {
    let note = editor
        .editor_state
        .data
        .get_note_view(note_idx)
        .expect("note 应存在");
    let view = &editor.editor_state.view;
    Point::new(
        view.tick_to_x(note.tick + note.length / 2.0),
        view.key_to_y(note.key) + view.zoom_y / 2.0,
    )
}

// ===== 优先级 1：选择框内部覆盖音符边缘命中 =====

#[test]
fn test_selection_inside_overrides_note_edge_hit() {
    // 有选中音符 + hit_result 命中音符 Start 边缘 + pos 在选择框内部
    // 应进入 DraggingSelection（不是 ResizingStart）
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(100.0, 60, 200.0)]);
    editor.editor_state.interaction.selected_notes.insert(0);

    let pos = pos_inside_selection(&editor, 0);
    let snapped_tick = editor.editor_state.data.get_note_view(0).unwrap().tick;

    // hit_result 模拟命中音符 Start 边缘（旧逻辑会走 ResizingStart）
    editor.handle_pointer_pressed(pos, Some((0, HitType::Start)), snapped_tick);

    assert!(
        matches!(
            editor.editor_state.interaction.edit_state,
            EditState::DraggingSelection { .. }
        ),
        "选择框内部应进入 DraggingSelection，而不是 ResizingStart"
    );
}

#[test]
fn test_selection_inside_overrides_note_middle_hit() {
    // 有选中音符 + hit_result 命中音符 Middle + pos 在选择框内部
    // 应进入 DraggingSelection（不是 PendingDrag）
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(100.0, 60, 200.0)]);
    editor.editor_state.interaction.selected_notes.insert(0);

    let pos = pos_inside_selection(&editor, 0);
    let snapped_tick = editor.editor_state.data.get_note_view(0).unwrap().tick;

    editor.handle_pointer_pressed(pos, Some((0, HitType::Middle)), snapped_tick);

    assert!(
        matches!(
            editor.editor_state.interaction.edit_state,
            EditState::DraggingSelection { .. }
        ),
        "选择框内部应进入 DraggingSelection，而不是 PendingDrag"
    );
}

// ===== 优先级 1：选择框边缘覆盖音符命中 =====

#[test]
fn test_selection_left_edge_overrides_note_hit() {
    // 有选中音符 + pos 在选择框左边缘 + hit_result 命中音符
    // 应进入 ResizingSelectionStart（不是单音符 ResizingStart）
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(100.0, 60, 200.0)]);
    editor.editor_state.interaction.selected_notes.insert(0);

    let pos = pos_at_left_edge(&editor, 0);
    let snapped_tick = editor.editor_state.data.get_note_view(0).unwrap().tick;

    editor.handle_pointer_pressed(pos, Some((0, HitType::Start)), snapped_tick);

    assert!(
        matches!(
            editor.editor_state.interaction.edit_state,
            EditState::ResizingSelectionStart { .. }
        ),
        "选择框左边缘应进入 ResizingSelectionStart"
    );
}

#[test]
fn test_selection_right_edge_overrides_note_hit() {
    // 有选中音符 + pos 在选择框右边缘 + hit_result 命中音符
    // 应进入 ResizingSelectionEnd
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(100.0, 60, 200.0)]);
    editor.editor_state.interaction.selected_notes.insert(0);

    let pos = pos_at_right_edge(&editor, 0);
    let note_view = editor.editor_state.data.get_note_view(0).unwrap();
    let snapped_tick = note_view.tick + note_view.length;

    editor.handle_pointer_pressed(pos, Some((0, HitType::End)), snapped_tick);

    assert!(
        matches!(
            editor.editor_state.interaction.edit_state,
            EditState::ResizingSelectionEnd { .. }
        ),
        "选择框右边缘应进入 ResizingSelectionEnd"
    );
}

// ===== 优先级 2：选择框外命中音符 → 单音符编辑 =====

#[test]
fn test_note_hit_outside_selection_box_switches_selection() {
    // 有选中音符 0 + 点击音符 1（在选择框外）+ hit_result 命中音符 1
    // 应切换选区到音符 1，进入单音符编辑
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(100.0, 60, 200.0),  // 音符 0
            Note::new(1000.0, 72, 200.0), // 音符 1，在音符 0 的选择框外
        ],
    );
    editor.editor_state.interaction.selected_notes.insert(0);

    let pos = pos_on_note_outside_selection(&editor, 1);
    let snapped_tick = editor.editor_state.data.get_note_view(1).unwrap().tick;

    editor.handle_pointer_pressed(pos, Some((1, HitType::Middle)), snapped_tick);

    // 应进入 PendingDrag（Middle 命中 → start_note_edit → PendingDrag）
    assert!(
        matches!(
            editor.editor_state.interaction.edit_state,
            EditState::PendingDrag { note_index: 1, .. }
        ),
        "选择框外命中音符应进入单音符 PendingDrag，得到 note_index=1"
    );
    // 选区应切换到音符 1
    assert!(
        editor.editor_state.interaction.selected_notes.contains(&1),
        "选区应切换到音符 1"
    );
    assert!(
        !editor.editor_state.interaction.selected_notes.contains(&0),
        "原选中音符 0 应被清除"
    );
}

// ===== 优先级 2：无选中音符 + 命中音符 → 单音符编辑 =====

#[test]
fn test_note_hit_when_no_selection() {
    // 无选中音符 + hit_result 命中音符 → 单音符编辑
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(100.0, 60, 200.0)]);
    // 不设置 selected_notes（为空）

    let pos = pos_on_note_outside_selection(&editor, 0);
    let snapped_tick = editor.editor_state.data.get_note_view(0).unwrap().tick;

    editor.handle_pointer_pressed(pos, Some((0, HitType::Middle)), snapped_tick);

    assert!(
        matches!(
            editor.editor_state.interaction.edit_state,
            EditState::PendingDrag { note_index: 0, .. }
        ),
        "无选中音符时命中音符应进入 PendingDrag"
    );
}

// ===== 优先级 3：都未命中 → 点击空白处开始新框选 =====

#[test]
fn test_blank_click_with_selection_commits_pending_and_starts_new_selection() {
    // 有选中音符 + 点击空白处（未命中选择框也未命中音符）
    // 应提交 pending（如果有）+ 清空选区 + 开始新框选
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(100.0, 60, 200.0)]);
    editor.editor_state.interaction.selected_notes.insert(0);

    // 点击在远离音符的空白处
    let view = &editor.editor_state.view;
    let pos = Point::new(
        view.tick_to_x(5000.0),
        view.key_to_y(80) + view.zoom_y / 2.0,
    );
    let snapped_tick = 5000.0;

    editor.handle_pointer_pressed(pos, None, snapped_tick);

    assert!(
        matches!(
            editor.editor_state.interaction.edit_state,
            EditState::Selecting { .. }
        ),
        "空白点击应进入 Selecting 状态"
    );
    assert!(
        editor.editor_state.interaction.selected_notes.is_empty(),
        "空白点击应清空选区"
    );
}

// ===== pending_drag_state 保留：选择框内部命中不清空 pending =====

#[test]
fn test_selection_inside_keeps_pending_drag_state() {
    // pending_drag_state 存在 + 点击选择框内部（累积模式）
    // 应保留 pending（不清空），进入 DraggingSelection
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(100.0, 60, 200.0)]);
    editor.editor_state.interaction.selected_notes.insert(0);

    // 设置 pending_drag_state（模拟之前拖动过且未提交）
    let mut pending = lumino_editor_state::DragState::from_single(0, 1, 0, 60);
    pending.set_delta(50, 2);
    editor.pending_drag_state = Some(pending);

    let pos = pos_inside_selection(&editor, 0);
    let snapped_tick = editor.editor_state.data.get_note_view(0).unwrap().tick;

    editor.handle_pointer_pressed(pos, Some((0, HitType::Middle)), snapped_tick);

    // 应进入 DraggingSelection
    assert!(
        matches!(
            editor.editor_state.interaction.edit_state,
            EditState::DraggingSelection { .. }
        ),
        "应进入 DraggingSelection"
    );
    // pending_drag_state 应保留（累积模式）
    assert!(
        editor.pending_drag_state.is_some(),
        "累积模式下 pending_drag_state 应保留"
    );
}
