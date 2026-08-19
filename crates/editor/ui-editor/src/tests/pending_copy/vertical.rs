use crate::EditState;
use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;

// ===== BUG 复现：Ctrl+拖动复制时「上下拖动」（key 偏移）复制不生效 =====
// 用户报告：按住 Ctrl 拖动批量框选内容时，上下拖动的复制不起作用——
// 音符表面上成功放置了（UI ghost 显示），但滚动一下又消失，内存没有数据。
// 根因：复制采用「延迟提交」（松手只保存 pending_copy_drag_state，点击
// 空白处才写入内存），用户复制后未点击空白处即滚动/查看 → 副本只在 UI 层、
// 内存无数据。
// 修复：复制改为**松手即提交**——副本立即写入 document（真实化），
// 滚动/切换视图后副本作为真实音符持续存在。
// 本测试模拟完整交互：按下 → 上下移动 → 松手，验证副本已真实化。

/// 完整交互模拟：seed → 选中 → Ctrl+按下选择框内部 → 上下移动 → 松手
fn full_ctrl_drag_vertical_copy(
    editor: &mut Editor,
    delta_keys: i16,
) -> Option<lumino_editor_state::DragState> {
    let (center_x, center_y, moved_y) = {
        let v = &editor.editor_state.view;
        let center_x = v.tick_to_x(240.0);
        let center_y = v.key_to_y(60) + v.zoom_y / 2.0;
        let moved_y = v.key_to_y((60 + delta_keys).clamp(0, 127) as u16) + v.zoom_y / 2.0;
        (center_x, center_y, moved_y)
    };

    // Ctrl+按下选择框内部 → DraggingSelectionCopy
    editor.set_ctrl_pressed(true);
    editor.handle_pressed(iced_core::Point::new(center_x, center_y), false);
    assert!(
        matches!(
            editor.editor_state.interaction.edit_state,
            EditState::DraggingSelectionCopy { .. }
        ),
        "Ctrl+按下应进入 DraggingSelectionCopy，实际 {:?}",
        editor.editor_state.interaction.edit_state
    );

    // 上下移动：key 60 → 60 + delta_keys
    editor.handle_moved(iced_core::Point::new(center_x, moved_y));

    // 拖动中 delta_key 应已更新
    if let EditState::DraggingSelectionCopy { drag_state } =
        &editor.editor_state.interaction.edit_state
    {
        assert_eq!(
            drag_state.delta_key, delta_keys,
            "拖动中 delta_key 应为 {}，实际 {}",
            delta_keys, drag_state.delta_key
        );
    }

    // 松手：副本立即写入内存（松手即提交）
    editor.handle_released();
    editor.pending_copy_drag_state.clone()
}

#[test]
fn test_repro_ctrl_drag_vertical_copy_up() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 2000.0;
    editor.editor_state.canvas.size_y = 4000.0;
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.editor_state.view.set_snap_precision(10.0);
    editor.selection_insert(0);

    let pending = full_ctrl_drag_vertical_copy(&mut editor, 3);
    // 松手即提交：pending 已清空，document 已有副本
    assert!(pending.is_none(), "松手即提交后 pending 应清空");
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        2,
        "副本应已写入内存（上下拖动复制生效）"
    );
    // 副本在 key 63、tick 不变
    let notes: Vec<_> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .collect();
    let copy = notes
        .iter()
        .find(|n| n.key == 63)
        .expect("副本（key 63）应已写入内存");
    assert_eq!(copy.start_tick, 0, "纯上下拖动 tick 不变");
    // 原件保留在 key 60
    assert!(notes.iter().any(|n| n.key == 60), "原件应保留");
}

#[test]
fn test_repro_ctrl_drag_vertical_copy_down() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 2000.0;
    editor.editor_state.canvas.size_y = 4000.0;
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.editor_state.view.set_snap_precision(10.0);
    editor.selection_insert(0);

    let pending = full_ctrl_drag_vertical_copy(&mut editor, -3);
    assert!(pending.is_none(), "松手即提交后 pending 应清空");
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        2,
        "向下拖动复制应写入内存"
    );
    let notes: Vec<_> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .collect();
    assert!(notes.iter().any(|n| n.key == 57), "副本（key 57）应已写入");
    assert!(notes.iter().any(|n| n.key == 60), "原件应保留");
}

/// 复现：上下拖动复制松手后，渲染层应仍能看到副本（滚动后不消失）
#[test]
fn test_repro_ctrl_drag_vertical_copy_visible_after_release() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 2000.0;
    editor.editor_state.canvas.size_y = 4000.0;
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.editor_state.view.set_snap_precision(10.0);
    editor.selection_insert(0);

    full_ctrl_drag_vertical_copy(&mut editor, 3);
    // 松手即提交后：pending 清空，副本是真实音符 → collect 走正常路径仍可见
    assert!(editor.pending_copy_drag_state.is_none());
    let mut visible: Vec<(f32, u16, f32)> = Vec::new();
    editor.collect_visible_note_data(&mut visible, None, 0.0);
    let keys: Vec<u16> = visible.iter().map(|(_, k, _)| *k).collect();
    assert!(
        keys.contains(&60),
        "原件（key 60）应仍可见，实际 {:?}",
        keys
    );
    assert!(
        keys.contains(&63),
        "副本（key 63）应仍可见（滚动后不消失），实际 {:?}",
        keys
    );
}

/// 复现：复制松手后滚动视口（scroll_y 变化），副本应保持可见
#[test]
fn test_repro_ctrl_drag_vertical_copy_visible_after_scroll() {
    let mut editor = Editor::new();
    editor.editor_state.canvas.size_x = 2000.0;
    editor.editor_state.canvas.size_y = 4000.0;
    test_helpers::seed_notes(&mut editor, 1, 0, &[Note::new(0.0, 60, 480.0)]);
    editor.editor_state.view.set_snap_precision(10.0);
    editor.selection_insert(0);

    full_ctrl_drag_vertical_copy(&mut editor, 3);
    assert!(editor.pending_copy_drag_state.is_none());

    // 模拟滚动：垂直滚动 1 个 key 距离（副本 key 63 仍在视口内）
    editor.editor_state.view.scroll_y = editor.editor_state.view.zoom_y;

    let mut visible: Vec<(f32, u16, f32)> = Vec::new();
    editor.collect_visible_note_data(&mut visible, None, 0.0);
    let keys: Vec<u16> = visible.iter().map(|(_, k, _)| *k).collect();
    assert!(
        keys.contains(&63),
        "滚动后副本（key 63）应仍可见，实际 {:?}",
        keys
    );
}
