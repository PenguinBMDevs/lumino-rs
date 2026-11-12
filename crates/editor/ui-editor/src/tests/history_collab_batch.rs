//! DRAW-035B：多步历史跳转的协作广播合并
//!
//! 目标：一次跳 N 步只产生 **1 批**广播，且广播内容（事件）与逐步
//! `undo()` 完全一致；被拒绝的跳转不得把会话留在批处理态。
//!
//! 注意：事件缓冲区是全局单例，测试并行时会互相污染；这里同时按
//! 「本测试专属的 tick 段 + key 段」过滤，避免偶发多算一条事件。

use crate::Editor;
use crate::Note;
use crate::tests::test_helpers::seed_notes;
use lumino_message::events::{Event, take_events, window};
use lumino_note_core::history::OpKind;

/// 3 步「新建音符」历史（协作同步已开启）
///
/// `base_tick` / `base_key` 为每个测试专属的坐标段，用于筛出本测试事件。
fn seeded_editor(base_tick: f32, base_key: u16) -> Editor {
    let mut editor = Editor::new();
    seed_notes(&mut editor, 1, 0, &[]);
    editor.editor_state.data.set_collab_sync_enabled(true);
    for i in 0..3u16 {
        editor
            .editor_state
            .data
            .push_history_with_op_kind(OpKind::NoteCreate);
        editor.editor_state.data.insert_note(
            editor.editor_state.data.current_track,
            Note::new(base_tick + f32::from(i) * 100.0, base_key + i, 80.0),
        );
    }
    editor
}

/// 取出与指定坐标段相关的协作同步事件（排序后便于逐项比较）
fn local_note_events(base_tick: f32, base_key: u16) -> Vec<String> {
    let key_range = base_key..=(base_key + 2);
    let tick_lo = base_tick.round() as i32;
    let tick_range = tick_lo..=(tick_lo + 200);
    let in_range = |tick: f32, key: u16| {
        key_range.contains(&key) && tick_range.contains(&(tick.round() as i32))
    };
    let mut events: Vec<String> = take_events()
        .into_iter()
        .filter_map(|event| match event {
            Event::Window(window::Event::Sync(window::sync::Event::LocalNoteAdded {
                tick,
                key,
                ..
            })) if in_range(tick, key) => Some(format!("add:{tick:.0}:{key}")),
            Event::Window(window::Event::Sync(window::sync::Event::LocalNoteDeleted {
                tick,
                key,
                ..
            })) if in_range(tick, key) => Some(format!("del:{tick:.0}:{key}")),
            _ => None,
        })
        .collect();
    events.sort();
    events
}

#[test]
fn test_history_jump_broadcast_is_merged_and_equivalent() {
    let base_tick = 9000.0;
    let base_key = 100;

    // 对照：逐步撤销 → 每步一批
    let mut single = seeded_editor(base_tick, base_key);
    let _ = local_note_events(base_tick, base_key);
    single.collab_broadcast_batches = 0;
    for _ in 0..3 {
        assert!(single.undo(), "逐步撤销应成功");
    }
    let single_events = local_note_events(base_tick, base_key);
    let single_batches = single.collab_broadcast_batches;

    // 目标：一次跳 3 步 → 只一批
    let mut batched = seeded_editor(base_tick, base_key);
    let _ = local_note_events(base_tick, base_key);
    batched.collab_broadcast_batches = 0;
    assert!(batched.history_jump_to(0), "跳转到初始状态应成功");
    let batched_events = local_note_events(base_tick, base_key);
    let batched_batches = batched.collab_broadcast_batches;

    assert_eq!(single_batches, 3, "对照：逐步撤销应为 3 批");
    assert_eq!(batched_batches, 1, "N 步跳转应合并为 1 批");
    assert_eq!(
        batched_events, single_events,
        "合并后的广播内容必须与逐步撤销一致（不丢、不重）"
    );
    assert_eq!(single_events.len(), 3, "三步撤销应广播 3 条删除");
    assert_eq!(batched.editor_state.data.current_track_note_count(), 0);
    assert_eq!(batched.collab_batch_depth, 0, "批处理深度必须复位");
}

#[test]
fn test_rejected_jump_leaves_batch_state_reset() {
    // 专属坐标段，避免与上一个测试的并行事件混淆
    let base_tick = 13000.0;
    let base_key = 30;
    let mut editor = seeded_editor(base_tick, base_key);
    let _ = local_note_events(base_tick, base_key);
    editor.collab_broadcast_batches = 0;

    assert!(!editor.history_jump_to(99), "越界目标必须拒绝");
    assert_eq!(editor.collab_batch_depth, 0, "拒绝路径不得残留批处理态");
    assert_eq!(editor.collab_broadcast_batches, 0, "拒绝路径不应产生广播");

    // 后续单步撤销仍应正常广播（没有卡在批处理态）
    assert!(editor.undo(), "单步撤销应成功");
    assert_eq!(editor.collab_broadcast_batches, 1);
}
