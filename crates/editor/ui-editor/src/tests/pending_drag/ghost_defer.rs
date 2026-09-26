use super::start_dragging_selection;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;

fn ticks(editor: &Editor) -> Vec<u32> {
    editor
        .editor_state
        .data
        .track_notes(1)
        .iter()
        .map(|n| n.start_tick)
        .collect()
}

// ===== 幽灵直到取消：框选拖动期间 document 不得落盘 =====

#[test]
fn test_batch_drag_ghost_until_cancel_no_doc_mutation() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        2,
        1,
        &[
            Note::from_raw(0.0, 60, 240.0, 100, 0),
            Note::from_raw(480.0, 62, 240.0, 100, 0),
        ],
    );
    editor.selection_insert(0);
    editor.selection_insert(1);
    assert_eq!(ticks(&editor), vec![0, 480]);

    // 活跃拖动中（鼠标按下未松手）：仅 ghost delta 变化，document 不得变动，
    // 不得启动异步提交，不得标记任何重传脏位。
    start_dragging_selection(&mut editor, [0, 1], 100, 0);
    assert_eq!(ticks(&editor), vec![0, 480], "活跃拖动中 document 不得变动");
    assert!(
        !editor.editor_state.data.has_pending_commit(),
        "活跃拖动中不得启动异步提交"
    );
    assert!(
        !editor.editor_state.data.note_delta_dirty,
        "活跃拖动中不得标记全量会话"
    );
    assert!(
        !editor.editor_state.data.main_track_struct_dirty,
        "活跃拖动中不得标记单轨重建"
    );

    // 松手（操作完成但未取消框选）：存 pending，仍保留选中，document 仍不得变动。
    editor.handle_released();
    assert!(editor.has_pending_drag(), "松手后应存在 pending");
    assert_eq!(
        editor.get_selected_indices().len(),
        2,
        "松手未取消时应保持选中姿势"
    );
    assert_eq!(
        ticks(&editor),
        vec![0, 480],
        "松手未取消时 document 不得落盘"
    );
    assert!(
        !editor.editor_state.data.has_pending_commit(),
        "松手仅存 pending，不得自动启动异步提交"
    );
    assert!(
        !editor.editor_state.data.note_delta_dirty,
        "pending 期间不得标记全量会话"
    );
    assert!(
        !editor.editor_state.data.main_track_struct_dirty,
        "pending 期间不得标记单轨重建"
    );
}

#[test]
fn test_batch_drag_accumulated_ghost_then_cancel_applies_once() {
    let mut editor = Editor::new();
    test_helpers::seed_notes(
        &mut editor,
        2,
        1,
        &[
            Note::from_raw(0.0, 60, 240.0, 100, 0),
            Note::from_raw(480.0, 62, 240.0, 100, 0),
        ],
    );
    editor.selection_insert(0);
    editor.selection_insert(1);

    // 第一次拖动 + 松手（pending delta=100，仍选中，未取消）
    start_dragging_selection(&mut editor, [0, 1], 100, 0);
    editor.handle_released();
    // 第二次拖动（累积，仍选中，未取消）：delta 累积到 150，document 仍不动
    start_dragging_selection(&mut editor, [0, 1], 50, 0);
    editor.handle_released();
    assert_eq!(
        editor
            .pending_drag_state
            .as_ref()
            .expect("pending 应存在")
            .delta_tick,
        150,
        "delta 应累积"
    );
    assert_eq!(ticks(&editor), vec![0, 480], "累积期间 document 不得落盘");
    assert!(
        !editor.editor_state.data.has_pending_commit(),
        "累积期间不得启动异步提交"
    );

    // 取消框选时落盘（模拟空白处点击 flush：提交 + 清空选区 + 等待完成）
    assert!(editor.commit_pending_drag(), "取消时应启动提交");
    // 异步飞行中 document 仍未变（后台线程未完成）
    assert!(
        editor.editor_state.data.has_pending_commit(),
        "提交后应有进行中的异步任务"
    );
    assert!(editor.drain_async_commit(), "落盘应有实际修改");
    assert_eq!(ticks(&editor), vec![150, 630], "取消后一次落盘累积 delta");
    assert!(
        !editor.editor_state.data.note_delta_dirty,
        "落盘走增量通道，不得触发全量会话"
    );
    editor.selection_clear();
    assert!(editor.get_selected_indices().is_empty(), "取消后选区应清空");
    assert!(editor.pending_drag_state.is_none());
}
