//! COW 语义与内存回归测试

use std::sync::Arc;

use super::util::{make_test_event, sorted_events};
use crate::chunked_list::{ChunkedList, EventTick};

/// COW 语义验证：clone 后修改互不干扰，且快照 clone 不拷贝数据
#[test]
fn test_clone_is_shallow_cow() {
    let mut original = ChunkedList::from_sorted(sorted_events(1000));
    let snapshot = original.clone(); // 浅拷贝：块 Arc 共享

    // 修改原容器（模拟 insert 到已快照的轨道）
    original.insert(make_test_event(5, 999));
    assert_eq!(original.len(), 1001);
    // 快照不受影响，且 token 顺序仍正确
    assert_eq!(snapshot.len(), 1000);
    assert_eq!(snapshot.first().expect("快照首元素应存在").tick, 0);
    let snapshot_ticks: Vec<u32> = snapshot.iter().map(EventTick::tick).collect();
    assert_eq!(
        snapshot_ticks,
        (0..1000).map(|i| i * 10).collect::<Vec<_>>()
    );
}

/// COW 语义验证：快照间相互修改互不影响
#[test]
fn test_two_snapshots_do_not_interfere() {
    let list = ChunkedList::from_sorted(sorted_events(1000));
    let snap_a = list.clone();
    let mut snap_b = list.clone();

    // 仅修改 snap_b，snap_a 与 list 都应保持
    snap_b.insert(make_test_event(15, 42));
    assert_eq!(list.len(), 1000);
    assert_eq!(snap_a.len(), 1000);
    assert_eq!(snap_b.len(), 1001);
    assert_eq!(list.to_vec(), snap_a.to_vec());
}

/// 内存回归验证：快照 clone 不复制数据（块 Arc 物理共享）。
///
/// Bug 1 根因：`make_snapshot` 曾全量 `to_vec()` 拷贝整轨为单一 Vec，
/// 1600W 音符工程快照 = 原始(256MB) + 快照(256MB) = 内存翻倍。
/// 修复后 `clone()` 为 O(块数) 指针拷贝——用 `Arc::strong_count` 直接断言
/// 快照与原始共享同一块分配，杜绝整轨数据复制。
#[test]
fn test_snapshot_clone_shares_blocks_no_copy() {
    // 覆盖多块场景（70 万事件 = 2 块），模拟 1600W 工程分块
    let mut original = ChunkedList::from_sorted(sorted_events(700_000));
    assert_eq!(original.chunk_count(), 2);

    // 记录每块的 Arc 引用计数基线（原始独占 → 均为 1）
    for arc in &original.chunks {
        assert_eq!(Arc::strong_count(arc), 1, "原始独占时块计数应为 1");
    }

    // 快照 = 浅拷贝（O(块数)），不复制任何音符数据
    let snapshot = original.clone();
    assert_eq!(snapshot.len(), 700_000);
    for arc in &original.chunks {
        assert_eq!(Arc::strong_count(arc), 2, "快照后块被共享而非复制");
    }

    // 修改原容器只复制目标块（COW）：其余块仍共享。
    // tick=5_000_001 落在尾块（范围 5_000_000..6_999_990）且尾块未满 → 只复制尾块
    original.insert(make_test_event(5_000_001, 999_999));
    assert_eq!(original.len(), 700_001);
    assert_eq!(snapshot.len(), 700_000, "快照不受修改影响");

    // 首块未修改 → 仍与快照物理共享（计数 = 2）
    assert_eq!(Arc::strong_count(&original.chunks[0]), 2);
    // 尾块被 make_mut 复制 → 计数回落为 1（COW 生效）
    assert_eq!(Arc::strong_count(&original.chunks[1]), 1);
    // 快照数据完整不受影响
    assert_eq!(snapshot.get(0).expect("快照索引 0 的事件应存在").tick, 0);
    assert_eq!(
        snapshot
            .iter()
            .map(EventTick::tick)
            .last()
            .expect("快照末元素应存在"),
        6_999_990
    );
}
