//! 端到端集成测试：ghost 拖动 → 提交 → 撤销 → 重做 完整流程
//!
//! 验证 DragState + Editor + History 三个模块的协作：
//! - 单音符拖动完整流程（push_history → 进入 Dragging → commit → undo → redo）
//! - 批量拖动完整流程
//! - 连续多次拖动的独立 undo 链
//! - 跨 group 逻辑撤销（直接调用 data.undo_logical）
//!
//! 2026-08 单一权威源：测试种子经 `test_helpers::seed_notes` 写入 document。

use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;
use lumino_note_core::history::OpKind;

/// 模拟用户完整拖动一个音符的流程：
/// push_history（按下时） → 进入 Dragging（移动时） → commit（松手时）
fn setup_dragging(editor: &mut Editor, note_index: usize, delta_tick: i64, delta_key: i16) {
    let note_count = editor.editor_state.data.current_track_note_count();
    let original_tick = editor
        .editor_state
        .data
        .get_note_view(note_index)
        .expect("note 应存在")
        .tick as i64;
    let original_key = editor
        .editor_state
        .data
        .get_note_view(note_index)
        .expect("note 应存在")
        .key as i16;

    // 模拟 try_start_drag：进入 Dragging
    // 注意：单音符拖动现在走 MoveOp，finalize_dragging 会自己 push 操作日志，
    // 这里不再额外 push 快照，避免两次拖动之间出现冗余快照。
    let mut drag = DragState::from_single(note_index, note_count, original_tick, original_key);
    drag.set_delta(delta_tick, delta_key);
    editor.editor_state.interaction.edit_state = EditState::Dragging {
        note_index,
        drag_state: drag,
        last_played_key: original_key as u16,
    };
}

/// 模拟用户完整批量拖动流程
fn setup_dragging_selection(
    editor: &mut Editor,
    indices: impl IntoIterator<Item = usize>,
    delta_tick: i64,
    delta_key: i16,
) {
    let note_count = editor.editor_state.data.current_track_note_count();

    // 模拟 pressed.rs 中的批量拖动入口：push_history + 进入 DraggingSelection
    editor.push_history();
    let mut drag = DragState::from_indices(indices, note_count, 0, 60);
    drag.set_delta(delta_tick, delta_key);
    editor.editor_state.interaction.edit_state = EditState::DraggingSelection { drag_state: drag };
}

// ===== 单音符拖动完整流程 =====

#[test]
fn test_single_note_drag_commit_undo_redo_flow() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // 拖动前：tick=0, key=60
    setup_dragging(&mut editor, 0, 100, 5);

    // 松手提交
    assert!(editor.commit_current_edit());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        100.0
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        65
    );

    // 撤销：恢复原位置
    assert!(editor.undo());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        0.0
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        60
    );

    // 重做：再次应用移动
    assert!(editor.redo());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        100.0
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        65
    );
}

#[test]
fn test_single_note_drag_with_clamp_undo_restores_original() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(50.0, 100, 480.0)]);

    // 拖动到 key=200（超过 max_key=127，应 clamp）
    setup_dragging(&mut editor, 0, 0, 100);

    assert!(editor.commit_current_edit());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        127,
        "应 clamp 到 127"
    );

    // 撤销应恢复到 key=100（原值），而不是 clamp 前的 200
    assert!(editor.undo());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        100
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        50.0
    );
}

// ===== 批量拖动完整流程 =====

#[test]
fn test_batch_drag_commit_undo_redo_flow() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        1,
        0,
        &[
            Note::new(0.0, 60, 240.0),
            Note::new(240.0, 62, 240.0),
            Note::new(480.0, 64, 240.0),
        ],
    );

    // 批量选中索引 0 和 2，拖动 +200, +7
    setup_dragging_selection(&mut editor, [0, 2], 200, 7);

    assert!(editor.commit_current_edit());
    let data = &editor.editor_state.data;
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        200.0
    );
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        67
    );
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").tick,
        240.0
    ); // 未选中，不变
    assert_eq!(
        data.get_note_view(1).expect("第 2 个音符视图应存在").key,
        62
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        680.0
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").key,
        71
    );

    // 撤销：所有选中音符恢复原位置
    assert!(editor.undo());
    let data = &editor.editor_state.data;
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        0.0
    );
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").key,
        60
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        480.0
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").key,
        64
    );

    // 重做
    assert!(editor.redo());
    let data = &editor.editor_state.data;
    assert_eq!(
        data.get_note_view(0).expect("第 1 个音符视图应存在").tick,
        200.0
    );
    assert_eq!(
        data.get_note_view(2).expect("第 3 个音符视图应存在").tick,
        680.0
    );
}

// ===== 连续多次拖动：每次拖动是独立的 undo 节点 =====

#[test]
fn test_multiple_drags_create_independent_undo_steps() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // 第一次拖动：+100, +5
    setup_dragging(&mut editor, 0, 100, 5);
    assert!(editor.commit_current_edit());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        100.0
    );

    // 第二次拖动：再 +50, +3
    setup_dragging(&mut editor, 0, 50, 3);
    assert!(editor.commit_current_edit());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        150.0
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        68
    );

    // 第一次撤销：回退第二次拖动
    assert!(editor.undo());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        100.0
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        65
    );

    // 第二次撤销：回退第一次拖动
    assert!(editor.undo());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        0.0
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        60
    );

    // 无法继续撤销
    assert!(!editor.can_undo());

    // 两次重做
    assert!(editor.redo());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        100.0
    );
    assert!(editor.redo());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        150.0
    );
}

// ===== 零 delta 拖动：不产生实际变更 =====

#[test]
fn test_zero_delta_drag_commit_undo_is_noop() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);

    // 零 delta 拖动
    setup_dragging(&mut editor, 0, 0, 0);

    // commit 仍然返回 true（is_editing=true），但 notes 不变
    assert!(editor.commit_current_edit());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        0.0
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        60
    );

    // 零 delta 不生成 MoveOp，也不 push 快照，因此没有可撤销的操作
    assert!(!editor.can_undo(), "零 delta 拖动不应产生历史记录");
    assert!(!editor.undo());
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .tick,
        0.0
    );
    assert_eq!(
        editor
            .editor_state
            .data
            .get_note_view(0)
            .expect("第 1 个音符视图应存在")
            .key,
        60
    );
}

// ===== 跨 group 逻辑撤销（直接调用 data.undo_logical）=====
//
// 此测试验证：当单个逻辑操作因超过 entry_limit 被分割为多个 group 时，
// `undo_logical` 能一次性回退整个逻辑操作链。

#[test]
fn test_cross_group_logical_undo_after_split() {
    let mut editor = Editor::new();

    // 2026-08 单一权威源：空 document 种子（后续 insert_note 写入当前轨）
    test_helpers::seed_notes(&mut editor, 1, 0, &[]);

    // 将 entry_limit 设为 3，merge_window=300ms，使得 4 次连续 NoteCreate 会触发分割
    // 注意：merge_window=0 表示不合并，必须 >0 才能触发合并/分割逻辑
    editor.editor_state.data.history.set_config(100, 300, 3);

    // 初始状态：0 个音符
    // 连续放置 4 个 NoteCreate（在合并窗口内）
    // **关键**：push_history_mergeable 必须在修改 notes 之前调用（与 finish_drawing 一致），
    // 这样快照捕获的是"操作前"状态，undo 时才能正确恢复
    for i in 0..4u16 {
        let _ = editor
            .editor_state
            .data
            .push_history_mergeable(OpKind::NoteCreate);
        editor.editor_state.data.insert_note(
            editor.editor_state.data.current_track,
            Note::new(i as f32 * 100.0, 60 + i, 80.0),
        );
    }

    // 现在有 4 个音符
    assert_eq!(editor.editor_state.data.current_track_note_count(), 4);
    // undo_stack 应有 2 个 group：group 1（3 entries）+ group 2（1 entry，parent=1）
    assert_eq!(editor.editor_state.data.history.undo_len(), 2);

    // 标准 undo 只回退一步（1 个音符）——验证分割确实发生了
    assert!(editor.editor_state.data.undo());
    assert_eq!(editor.editor_state.data.current_track_note_count(), 3);

    // 此时 undo_stack 只剩 group 1（3 entries，notes=[]）
    assert_eq!(editor.editor_state.data.history.undo_len(), 1);

    // 逻辑撤销：group 1 的快照 notes=[]（chain 开始前的状态），
    // undo_logical 应一次性回退整个 chain，恢复到 0 个音符
    assert!(editor.editor_state.data.undo_logical());
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        0,
        "逻辑撤销应回退剩余 NoteCreate chain，恢复到 0 个音符"
    );
}
