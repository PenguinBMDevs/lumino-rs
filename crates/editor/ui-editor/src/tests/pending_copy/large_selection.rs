//! 大批量选中 + Ctrl 拖动复制回归测试（跨位图块）
//!
//! **BUG**：`DragState::selected_indices_fast()` 曾以 `block_idx * 64` 计算块基址，
//! 而 `bit_vec::BitVec` 的默认块类型是 **`u32`（32 位）**。选中音符跨过第 32 位后，
//! 返回的索引整体偏移 +32/块，`commit_pending_copy` 据此取音符：
//! - 偏移后越界 → 副本集合为空 → **完全无法批量复制**；
//! - 偏移后落在别的音符上 → 复制出未选中的音符 → **复制体散开、飘到别处**。
//! 拖动中的 ghost 预览用的是位图直索引（正确），所以现象是「看着是整体平移，
//! 松手后副本散开/消失」。
//!
//! 本文件用 ≥ 100 个音符（位图跨越 4 个块）锁定该性质：副本必须是对原选区的
//! **刚性平移**（所有音符共享同一 delta）。

use crate::Editor;
use crate::note::Note;
use crate::tests::test_helpers;
use lumino_editor_state::DragState;
use lumino_ui_core::message::{EditorAction, Point2};

/// 10 行（key 50..59） × 10 列（tick 0,60,...,540）= 100 个音符，位图跨 4 个 32 位块。
fn seed_grid(editor: &mut Editor) {
    let mut notes = Vec::with_capacity(100);
    for row in 0..10u16 {
        for col in 0..10u32 {
            notes.push(Note::new(col as f32 * 60.0, 50 + row, 60.0));
        }
    }
    editor.editor_state.canvas.size_x = 2000.0;
    editor.editor_state.canvas.size_y = 4000.0;
    test_helpers::seed_notes(editor, 1, 0, &notes);
    editor.editor_state.view.set_snap_precision(10.0);
}

/// 当前轨 (start_tick, key) 升序列表
fn tick_keys(editor: &Editor) -> Vec<(u32, u8)> {
    let mut v: Vec<(u32, u8)> = editor
        .editor_state
        .data
        .current_track_notes()
        .iter()
        .map(|n| (n.start_tick, n.key))
        .collect();
    v.sort();
    v
}

/// `all - originals`（本测试 delta 使原件与副本 tick 区间互不重叠，集合差即可）
fn extras(all: &[(u32, u8)], orig: &[(u32, u8)]) -> Vec<(u32, u8)> {
    let mut out = Vec::new();
    let mut used = vec![false; all.len()];
    for o in orig {
        let mut found = None;
        for (i, n) in all.iter().enumerate() {
            if !used[i] && n == o {
                found = Some(i);
                break;
            }
        }
        used[found.expect("原件应仍存在")] = true;
    }
    for (i, n) in all.iter().enumerate() {
        if !used[i] {
            out.push(*n);
        }
    }
    out.sort();
    out
}

#[test]
fn test_ctrl_copy_large_selection_places_copies_at_uniform_delta() {
    let mut editor = Editor::new();
    seed_grid(&mut editor);
    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        100,
        "前置：应种子 100 个音符"
    );

    // 全选 100 个（选中位图跨越 4 个 u32 块——正是失效区间）
    editor.select_all_notes();
    assert_eq!(editor.get_selected_indices().len(), 100);

    // 直接构造复制拖拽（delta = +700 tick / +5 key，与原件 tick 区间不重叠）并提交
    let count = editor.editor_state.data.current_track_note_count();
    let mut ds = DragState::from_indices(editor.get_selected_indices(), count, 0, 60);
    ds.set_delta(700, 5);
    editor.pending_copy_drag_state = Some(ds);
    assert!(editor.commit_pending_copy(), "副本应写入内存层");

    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        200,
        "应写入 100 个副本（漏写 = 无法批量复制）"
    );

    let orig = tick_keys(&editor)
        .into_iter()
        .filter(|(t, _)| *t < 600)
        .collect::<Vec<_>>();
    let all = tick_keys(&editor);
    let copies = extras(&all, &orig);
    assert_eq!(copies.len(), 100, "副本数量应为 100，实际 {}", copies.len());

    // 关键性质：副本必须是原选区的**刚性平移**（同一 delta），
    // 逐个比对——任何「散开/偏移」都会在这里暴露。
    for (o, c) in orig.iter().zip(copies.iter()) {
        assert_eq!(
            (c.0 as i64 - o.0 as i64, c.1 as i32 - o.1 as i32),
            (700, 5),
            "副本 {:?} 相对原件 {:?} 的偏移应为 (700, 5)",
            c,
            o
        );
    }
}

#[test]
fn test_ctrl_copy_large_selection_full_action_flow() {
    let mut editor = Editor::new();
    seed_grid(&mut editor);
    editor.select_all_notes();
    let orig = tick_keys(&editor);
    assert_eq!(orig.len(), 100);

    // 选择框中心按下（Ctrl 经 canvas 通道上报）
    let (cx, cy) = {
        let (x1, x2, y1, y2) = editor
            .get_selection_box_bounds()
            .expect("有选中音符时应能计算选择框");
        ((x1 + x2) / 2.0, (y1 + y2) / 2.0)
    };
    editor.handle_action(EditorAction::Pressed {
        pos: Point2::new(cx, cy),
        shift: false,
        ctrl: true,
    });
    assert!(
        matches!(
            editor.editor_state.interaction.edit_state,
            crate::EditState::DraggingSelectionCopy { .. }
        ),
        "Ctrl+选择框内部按下应进入复制拖拽，实际 {:?}",
        editor.editor_state.interaction.edit_state
    );

    // 右移 700 tick、上移 5 key（key 越大 y 越小），松手即提交
    let (zoom_x, zoom_y) = {
        let v = &editor.editor_state.view;
        (v.zoom_x, v.zoom_y)
    };
    editor.handle_action(EditorAction::Moved(Point2::new(
        cx + 700.0 * zoom_x,
        cy - 5.0 * zoom_y,
    )));
    editor.handle_action(EditorAction::Released);

    assert_eq!(
        editor.editor_state.data.current_track_note_count(),
        200,
        "完整交互链路后应写入 100 个副本"
    );

    let all = tick_keys(&editor);
    let copies = extras(&all, &orig);
    assert_eq!(copies.len(), 100, "副本数量应为 100，实际 {}", copies.len());

    // 刚性平移断言：所有副本共享同一 delta（散开会在此失败）
    let delta = (
        copies[0].0 as i64 - orig[0].0 as i64,
        copies[0].1 as i32 - orig[0].1 as i32,
    );
    for (o, c) in orig.iter().zip(copies.iter()) {
        assert_eq!(
            (c.0 as i64 - o.0 as i64, c.1 as i32 - o.1 as i32),
            delta,
            "副本 {:?} 相对原件 {:?} 的偏移必须与其余副本一致（不得散开）",
            c,
            o
        );
    }
    assert_eq!(delta, (700, 5), "整体偏移应为 (700, 5)，实际 {:?}", delta);
}
