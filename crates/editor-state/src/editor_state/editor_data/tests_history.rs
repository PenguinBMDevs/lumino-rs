//! Undo/Redo 与 COW Arc 共享测试

use std::sync::Arc;

use lumino_note_core::automation::{AutomationEdit, AutomationTarget, SegmentShape};
use lumino_note_core::history::CreateOp;
use lumino_note_core::note::Note;

use super::EditorData;
use super::accessors::note_to_event;

#[test]
fn test_undo_redo_basic() {
    // 用空 document 构造（track 1），快照空状态后经 insert_note 写入
    let mut data = EditorData::with_f32_notes(1, &[]);
    data.push_history();
    data.insert_note(data.current_track, Note::new(0.0, 60, 1.0));
    assert_eq!(data.current_track_note_count(), 1);
    assert!(data.can_undo());

    let undone = data.undo();
    assert!(undone);
    assert_eq!(
        data.current_track_note_count(),
        0,
        "undo should restore empty notes"
    );
    assert!(data.can_redo());

    let redone = data.redo();
    assert!(redone);
    assert_eq!(
        data.current_track_note_count(),
        1,
        "redo should restore the note"
    );
}

#[test]
fn test_undo_when_nothing_to_undo() {
    let mut data = EditorData::new();
    assert!(!data.can_undo());
    assert!(!data.undo(), "undo on empty history = false");
}

// ── COW / Arc 共享测试 ──

#[test]
fn test_automation_lane_cow_shares_unmodified_lanes() {
    let mut data = EditorData::new();
    data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 7 });
    data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 1 });

    // 快照——所有 lane 的 Arc refcount +1
    data.push_history();

    // 记录 lane 0 的 Arc 地址
    let lane0_ptr = Arc::as_ptr(&data.automation_lanes[0]);

    // 修改 lane 1——只有 lane 1 触发 COW（Arc::make_mut 复制 lane 1）
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 1 },
        channel: 0,
        tick: 100,
        value: 64,
        shape: SegmentShape::Step,
    });

    // lane 0 未被修改→地址不变（物理共享）
    assert_eq!(
        lane0_ptr,
        Arc::as_ptr(&data.automation_lanes[0]),
        "未修改的 lane 必须在快照前后共享同一 Arc 分配"
    );
    // lane 0 的数据也不变
    assert_eq!(
        data.automation_lanes[0].target,
        AutomationTarget::CC { controller: 7 }
    );
}

#[test]
fn test_automation_lane_undo_restores_data() {
    let mut data = EditorData::new();
    data.find_or_create_automation_lane(0, AutomationTarget::CC { controller: 7 });
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 100,
        value: 64,
        shape: SegmentShape::Step,
    });

    // 快照（1 lane, 1 event）
    data.push_history();

    // 添加第二个事件
    data.apply_automation_edit(AutomationEdit::Add {
        track_idx: 0,
        target: AutomationTarget::CC { controller: 7 },
        channel: 0,
        tick: 200,
        value: 127,
        shape: SegmentShape::Step,
    });
    assert_eq!(data.automation_lanes[0].events.len(), 2);

    // 撤销——回到 1 event
    assert!(data.undo());
    assert_eq!(data.automation_lanes[0].events.len(), 1);
    assert_eq!(data.automation_lanes[0].events[0].tick, 100);

    // 重做——回到 2 events
    assert!(data.redo());
    assert_eq!(data.automation_lanes[0].events.len(), 2);
}

// ── 音符创建增量日志（NoteCreate 极简化）────────────────────────

#[test]
fn test_finish_drawing_incremental_create_log() {
    let mut data = EditorData::with_f32_notes(1, &[]);

    let note = data
        .finish_drawing(0.0, 60, 80.0, 1.0, 80.0)
        .expect("铅笔绘制应创建音符");
    assert_eq!(note.tick, 0.0);
    assert_eq!(note.key, 60);
    assert_eq!(data.current_track_note_count(), 1);

    // 历史栈顶应是轻量 CreateEntry（非快照）
    let back = data.history.undo_back().expect("应有历史条目");
    assert!(
        matches!(back, lumino_note_core::history::HistoryEntry::Create(_)),
        "铅笔绘制应走增量 CreateOp 日志而非整轨快照"
    );

    // undo：按值精确删除音符
    assert!(data.undo());
    assert_eq!(data.current_track_note_count(), 0);
    // redo：按 tick 有序重新插入
    assert!(data.redo());
    assert_eq!(data.current_track_note_count(), 1);
    let restored = data.current_track_notes().get(0).unwrap();
    assert_eq!(restored.key, 60);
    assert_eq!(restored.start_tick, 0);
}

#[test]
fn test_finish_drawing_logical_undo_across_split() {
    let mut data = EditorData::with_f32_notes(1, &[]);
    data.history.set_config(100, 300, 3);

    // 4 次连续绘制：300ms 窗口内合并 → 组1（3 条）+ 组2（1 条，parent=组1）
    for i in 0..4u16 {
        data.finish_drawing(i as f32 * 100.0, 60, i as f32 * 100.0 + 80.0, 1.0, 80.0);
    }
    assert_eq!(data.current_track_note_count(), 4);
    assert_eq!(data.history.undo_len(), 2, "应分割为 2 个分组");

    // 逻辑撤销：跨 chain 一次性回退全部 4 个音符
    assert!(data.undo_logical());
    assert_eq!(
        data.current_track_note_count(),
        0,
        "逻辑撤销应回退整个 NoteCreate chain"
    );

    // 逻辑重做：全部恢复
    assert!(data.redo_logical());
    assert_eq!(data.current_track_note_count(), 4);
}

#[test]
fn test_apply_create_ops_by_value_exact_undo() {
    let mut data = EditorData::with_f32_notes(1, &[]);

    // 两个同 tick 不同 key 的音符（position_of 必须按值精确匹配）
    data.insert_note(data.current_track, Note::new(0.0, 60, 1.0));
    data.insert_note(data.current_track, Note::new(0.0, 72, 1.0));
    assert_eq!(data.current_track_note_count(), 2);

    let e1 = note_to_event(Note::new(0.0, 60, 1.0));
    let e2 = note_to_event(Note::new(0.0, 72, 1.0));
    let ops = vec![
        CreateOp {
            track_id: 1,
            note: e1,
        },
        CreateOp {
            track_id: 1,
            note: e2,
        },
    ];

    // undo 第二个 op（key=72）——必须精确删除 key=72 而非 key=60
    assert_eq!(data.apply_create_ops(&ops[1..], true), 1);
    assert_eq!(data.current_track_note_count(), 1);
    assert_eq!(
        data.current_track_notes().get(0).unwrap().key,
        60,
        "按值删除必须匹配正确的音符"
    );

    // 再 undo 第一个 op
    assert_eq!(data.apply_create_ops(&ops[..1], true), 1);
    assert_eq!(data.current_track_note_count(), 0);

    // redo 全部重新插入
    assert_eq!(data.apply_create_ops(&ops, false), 2);
    assert_eq!(data.current_track_note_count(), 2);
    // 恢复后保持 tick 有序（insert_note 有序插入）
    assert_eq!(data.current_track_notes().get(0).unwrap().key, 60);
    assert_eq!(data.current_track_notes().get(1).unwrap().key, 72);
}
