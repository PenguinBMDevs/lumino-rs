//! `TickIndexedEvents::position_of_unused` 按值定位测试（同值多份按份数分配）
//!
//! 重点覆盖三件事：
//! 1. **份数分配**：同值多份必须逐份分配不同索引，否则同值音符只选中一份（静默丢份）；
//! 2. **存储等价**：`ChunkedList<T>`（分块）与 `[T]`（连续切片）行为必须完全一致——
//!    二者共用同一份算法，本测试是「唯一实现」不退化为两份的守门人；
//! 3. **无跨段兜底**：同 tick 段内未命中即 `None`，不得扫到相邻 tick 段。

use std::collections::HashSet;
use std::sync::Arc;

use super::util::{TestEvent, make_test_event};
use crate::chunked_list::{ChunkedList, TickIndexedEvents};

/// 同值三份：逐次定位必须分配 0/1/2，用尽后返回 None
#[test]
fn test_duplicate_values_get_distinct_indices() {
    let events = vec![
        make_test_event(10, 1),
        make_test_event(10, 1),
        make_test_event(10, 1),
    ];
    let list = ChunkedList::from_sorted(events);
    let target = make_test_event(10, 1);

    let mut used = HashSet::new();
    let mut got = Vec::new();
    for round in 0..3 {
        let idx = list
            .position_of_unused(&target, &used)
            .unwrap_or_else(|| panic!("第 {} 份应命中", round + 1));
        used.insert(idx);
        got.push(idx);
    }
    assert_eq!(got, vec![0, 1, 2], "同值三份应分配 0/1/2");
    assert!(
        list.position_of_unused(&target, &used).is_none(),
        "三份用尽后应返回 None（不得重复分配已占用索引）"
    );
}

/// used 中已占用的索引必须跳过，改选同段下一份
#[test]
fn test_used_index_skipped() {
    let events = vec![make_test_event(5, 1), make_test_event(5, 1)];
    let slice: &[TestEvent] = &events;
    let target = make_test_event(5, 1);

    let mut used = HashSet::new();
    used.insert(0); // 手动占用第 0 份
    assert_eq!(
        slice.position_of_unused(&target, &used),
        Some(1),
        "已占用 0 应改选第 1 份"
    );
    used.insert(1);
    assert_eq!(
        slice.position_of_unused(&target, &used),
        None,
        "两份均占用应返回 None"
    );
}

/// 同 tick 段内多事件：只命中全字段相等者，段内无匹配即 None（不跨段兜底）
#[test]
fn test_segment_scan_matches_exact_value_only() {
    let events = vec![
        make_test_event(10, 1),
        make_test_event(10, 2),
        make_test_event(10, 3),
        make_test_event(20, 9),
    ];
    let slice: &[TestEvent] = &events;
    let used = HashSet::new();

    assert_eq!(
        slice.position_of_unused(&make_test_event(10, 2), &used),
        Some(1),
        "段内应精确定位到全字段相等者"
    );
    assert_eq!(
        slice.position_of_unused(&make_test_event(10, 9), &used),
        None,
        "tick 10 段内无 (10,9)；tick 20 段虽有同 id 但不同段，不得跨段命中"
    );
    assert_eq!(
        slice.position_of_unused(&make_test_event(99, 1), &used),
        None,
        "超出全部事件范围应返回 None"
    );
}

/// 空序列：任何目标都返回 None（不越界、不 panic）
#[test]
fn test_empty_sequence_returns_none() {
    let list: ChunkedList<TestEvent> = ChunkedList::new();
    let empty: &[TestEvent] = &[];
    let target = make_test_event(0, 0);
    let used = HashSet::new();

    assert_eq!(list.position_of_unused(&target, &used), None);
    assert_eq!(empty.position_of_unused(&target, &used), None);
}

/// 分块存储：同值多份跨块时全局索引仍正确，且不串到相邻 tick 段
#[test]
fn test_chunked_duplicates_across_chunks() {
    // 手工构造两块（绕过 50 万真实容量）：块 0 = tick 10 两份；块 1 = tick 20 两份
    let mut list: ChunkedList<TestEvent> = ChunkedList::new();
    list.chunks.push(Arc::new(vec![
        make_test_event(10, 7),
        make_test_event(10, 7),
    ]));
    list.chunks.push(Arc::new(vec![
        make_test_event(20, 7),
        make_test_event(20, 7),
    ]));
    list.total_len = 4;
    list.rebuild_index();

    let target = make_test_event(10, 7);
    let mut used = HashSet::new();
    let a = list.position_of_unused(&target, &used).expect("第 1 份");
    used.insert(a);
    let b = list.position_of_unused(&target, &used).expect("第 2 份");
    used.insert(b);
    assert_eq!((a, b), (0, 1), "两份均应在块 0 内，全局索引 0/1");
    assert!(
        list.position_of_unused(&target, &used).is_none(),
        "tick 10 段用尽后不得跨到块 1 的 tick 20 段"
    );
}

/// **存储等价守门人**：多种目标、逐轮占用索引，`ChunkedList` 与 `[T]` 结果必须逐步一致
#[test]
fn test_chunked_and_slice_behaviour_identical() {
    let events = vec![
        make_test_event(0, 1),
        make_test_event(10, 2),
        make_test_event(10, 2), // 同值多份
        make_test_event(10, 3),
        make_test_event(20, 2),
    ];
    let list = ChunkedList::from_sorted(events.clone());
    let slice: &[TestEvent] = &events;

    let targets = [
        make_test_event(10, 2),  // 同值两份
        make_test_event(10, 3),  // 段内唯一
        make_test_event(20, 2),  // 后段同 id
        make_test_event(10, 99), // 段内未命中
        make_test_event(99, 1),  // 越界
    ];

    for target in targets {
        let mut used_list = HashSet::new();
        let mut used_slice = HashSet::new();
        // 每轮各占用一个索引，覆盖「份数分配」路径
        for round in 0..3 {
            let from_list = list.position_of_unused(&target, &used_list);
            let from_slice = slice.position_of_unused(&target, &used_slice);
            assert_eq!(
                from_list, from_slice,
                "第 {round} 轮 target={target:?}：分块与切片结果不一致（唯一实现被破坏）"
            );
            if let Some(i) = from_list {
                used_list.insert(i);
            }
            if let Some(i) = from_slice {
                used_slice.insert(i);
            }
        }
    }
}
