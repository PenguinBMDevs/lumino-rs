//! 合并窗口（push_mergeable）与 set_config 运行时配置测试
//!
//! 覆盖：
//! - 窗口内合并 / 超限分割（parent_group_id 指向旧 group）
//! - 不同 op_kind 不合并 / 窗口=0 永不合并
//! - set_config 运行时裁剪栈 / 合并参数更新

use super::{assert_snapshot, make_snapshot};
use crate::history::{History, OpKind};

#[test]
fn test_push_mergeable_within_window_merges() {
    // 配置：合并窗口 1000ms（足够大），单条上限 1000
    let mut history = History::with_config(100, 1000, 1000);

    // 第一次 push：新增
    let merged = history.push_mergeable(make_snapshot(1), OpKind::NoteCreate);
    assert!(!merged, "首次 push 不应合并");
    assert_eq!(history.undo_len(), 1);
    assert_eq!(
        assert_snapshot(history.undo_back().expect("应存在可撤销的历史条目")).entry_count,
        1
    );

    // 第二次 push：合并到栈顶，entry_count + 1，notes 保留 snap1
    let merged = history.push_mergeable(make_snapshot(2), OpKind::NoteCreate);
    assert!(merged, "同 op_kind + 窗口内 + 未超限应合并");
    assert_eq!(history.undo_len(), 1, "合并后栈大小不变");
    assert_eq!(
        assert_snapshot(history.undo_back().expect("应存在可撤销的历史条目")).entry_count,
        2
    );

    // 第三次 push：继续合并
    history.push_mergeable(make_snapshot(3), OpKind::NoteCreate);
    assert_eq!(history.undo_len(), 1);
    assert_eq!(
        assert_snapshot(history.undo_back().expect("应存在可撤销的历史条目")).entry_count,
        3
    );
    assert_eq!(
        assert_snapshot(history.undo_back().expect("应存在可撤销的历史条目"))
            .notes
            .len(),
        1,
        "合并后快照内容应保留 chain 中最早操作之前的状态（snap1）"
    );
}

#[test]
fn test_push_mergeable_exceeds_limit_splits_with_parent() {
    // 配置：合并窗口 1000ms，单条上限 2
    let mut history = History::with_config(100, 1000, 2);

    history.push_mergeable(make_snapshot(1), OpKind::NoteCreate);
    history.push_mergeable(make_snapshot(2), OpKind::NoteCreate);
    assert_eq!(
        assert_snapshot(history.undo_back().expect("应存在可撤销的历史条目")).entry_count,
        2,
        "应已合并到上限"
    );
    let old_group_id = assert_snapshot(history.undo_back().expect("应存在可撤销的历史条目"))
        .group_id
        .expect("撤销快照的 group_id 应存在");

    // 第三次 push：应分割为新 group，parent 指向旧 group
    let merged = history.push_mergeable(make_snapshot(3), OpKind::NoteCreate);
    assert!(!merged, "超限分割不算合并");
    assert_eq!(history.undo_len(), 2, "分割后栈大小 +1");

    let top = assert_snapshot(history.undo_back().expect("应存在可撤销的历史条目"));
    assert_eq!(top.entry_count, 1, "新分割组 entry_count 重置为 1");
    assert!(
        top.parent_group_id.is_some(),
        "分割组必须有 parent_group_id 指向被分割的旧 group"
    );
    assert_eq!(
        top.parent_group_id.expect("顶层条目应有父分组"),
        old_group_id,
        "parent_group_id 必须指向被分割的旧 group_id"
    );
    assert_ne!(
        top.group_id.expect("顶层条目应属于某分组"),
        old_group_id,
        "新分割组必须分配新 group_id"
    );
}

#[test]
fn test_push_mergeable_different_kind_no_merge() {
    let mut history = History::with_config(100, 1000, 1000);

    history.push_mergeable(make_snapshot(1), OpKind::NoteCreate);
    // 不同 op_kind 不应合并
    let merged = history.push_mergeable(make_snapshot(2), OpKind::NoteDelete);
    assert!(!merged, "不同 op_kind 必须不合并");
    assert_eq!(history.undo_len(), 2);
}

#[test]
fn test_push_mergeable_zero_window_never_merges() {
    // 合并窗口 = 0：任何两次连续 push 都不可能在窗口内
    let mut history = History::with_config(100, 0, 1000);

    history.push_mergeable(make_snapshot(1), OpKind::NoteCreate);
    let merged = history.push_mergeable(make_snapshot(2), OpKind::NoteCreate);
    assert!(!merged, "窗口=0 时不应合并");
    assert_eq!(history.undo_len(), 2);
}

// ── set_config 测试 ──────────────────────────────────────────

#[test]
fn test_set_config_trims_stack() {
    let mut history = History::with_config(100, 300, 1000);
    for i in 1..=10 {
        history.push(make_snapshot(i));
    }
    assert_eq!(history.undo_len(), 10);

    // 收紧上限到 5：应立即裁剪
    history.set_config(5, 300, 1000);
    assert_eq!(history.undo_len(), 5, "set_config 应立即裁剪栈");
    // 最早的 1-5 被弹出，栈顶是 10
    assert_eq!(
        assert_snapshot(history.undo_back().expect("应存在可撤销的历史条目"))
            .notes
            .len(),
        10
    );
}

#[test]
fn test_set_config_updates_merge_params() {
    let mut history = History::with_config(100, 1000, 1000);
    history.set_config(100, 0, 1);
    // 窗口=0 + 单条上限=1：不可能合并，第二次 push 必然分割
    history.push_mergeable(make_snapshot(1), OpKind::NoteCreate);
    history.push_mergeable(make_snapshot(2), OpKind::NoteCreate);
    assert_eq!(history.undo_len(), 2, "窗口=0 + 上限=1 时每次 push 都新增");
}
