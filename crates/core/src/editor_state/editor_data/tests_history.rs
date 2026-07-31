//! Undo/Redo 与 COW Arc 共享测试

use std::sync::Arc;

use crate::automation::{AutomationEdit, AutomationTarget, SegmentShape};
use crate::note::Note;

use super::EditorData;

#[test]
fn test_undo_redo_basic() {
    let mut data = EditorData::new();
    data.push_history();
    data.notes.push_back(Note::new(0.0, 60, 1.0));
    assert_eq!(data.notes.len(), 1);
    assert!(data.can_undo());

    let undone = data.undo();
    assert!(undone);
    assert!(data.notes.is_empty(), "undo should restore empty notes");
    assert!(data.can_redo());

    let redone = data.redo();
    assert!(redone);
    assert_eq!(data.notes.len(), 1, "redo should restore the note");
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
