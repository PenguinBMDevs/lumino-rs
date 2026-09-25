//! 有序不变式恢复（`restore_sorted` / `restore_sorted_vec`）单元测试
//!
//! 背景：拖动流式应用 / undo-redo MoveOp / 量化等路径为性能就地改 tick，
//! 破坏「按 tick 升序」后 `partition_point` / `window_range` 会漏检音符。

use super::util::{make_test_event, multi_chunk, sorted_events};
use crate::chunked_list::{ChunkedList, EventTick, restore_sorted_vec, sorted_locally_in_slice};

/// 就地改 tick（模拟拖动/量化路径的 `get_mut` 写）
fn set_tick(list: &mut ChunkedList<super::util::TestEvent>, idx: usize, tick: u32) {
    if let Some(e) = list.get_mut(idx) {
        e.tick = tick;
    }
}

fn ticks(list: &ChunkedList<super::util::TestEvent>) -> Vec<u32> {
    list.iter().map(EventTick::tick).collect()
}

fn is_sorted(list: &ChunkedList<super::util::TestEvent>) -> bool {
    ticks(list).windows(2).all(|w| w[0] <= w[1])
}

#[test]
fn test_restore_sorted_small_path_reorders() {
    let mut list = ChunkedList::from_sorted(sorted_events(5)); // 0,10,20,30,40
    set_tick(&mut list, 0, 35); // 35,10,20,30,40 → 需重排
    assert!(list.restore_sorted(&[0]), "失序应返回 true");
    assert!(is_sorted(&list));
    assert_eq!(ticks(&list), vec![10, 20, 30, 35, 40]);
}

#[test]
fn test_restore_sorted_noop_when_still_sorted() {
    let mut list = ChunkedList::from_sorted(sorted_events(5));
    set_tick(&mut list, 2, 25); // 0,10,25,30,40 仍有序
    assert!(!list.restore_sorted(&[2]), "仍有序应返回 false（零重排）");
    assert_eq!(ticks(&list), vec![0, 10, 25, 30, 40]);
}

#[test]
fn test_restore_sorted_moves_note_to_front() {
    let mut list = ChunkedList::from_sorted(sorted_events(4)); // 0,10,20,30
    set_tick(&mut list, 3, 5); // 5 移到最前：0,10,20,5
    assert!(list.restore_sorted(&[3]));
    assert_eq!(ticks(&list), vec![0, 5, 10, 20]);
}

#[test]
fn test_restore_sorted_large_path_merge() {
    // 20 个元素（> 小集合阈值 16）→ 稳定归并重建路径
    let mut list = ChunkedList::from_sorted(sorted_events(20));
    let before_ids: Vec<u32> = list.iter().map(|e| e.id).collect();
    // 前 5 个搬到所有元素之后
    for (i, t) in [(0usize, 300u32), (1, 310), (2, 320), (3, 330), (4, 340)] {
        set_tick(&mut list, i, t);
    }
    let moved: Vec<usize> = (0..20).collect();
    assert!(list.restore_sorted(&moved));
    assert!(is_sorted(&list));
    let mut after_ids: Vec<u32> = list.iter().map(|e| e.id).collect();
    let mut before_sorted = before_ids;
    before_sorted.sort_unstable();
    after_ids.sort_unstable();
    assert_eq!(after_ids, before_sorted, "元素多重集必须保持不变");
    assert_eq!(
        ticks(&list)[15..].to_vec(),
        vec![300, 310, 320, 330, 340],
        "被移动元素应落在末尾"
    );
}

#[test]
fn test_restore_sorted_ties_keep_unmoved_first() {
    // 未移动元素与移动元素同 tick：未移动在前（等价有序插入的稳定语义）
    let mut list = ChunkedList::from_sorted(vec![
        make_test_event(0, 0),
        make_test_event(10, 1),
        make_test_event(20, 2),
    ]);
    set_tick(&mut list, 2, 0); // id2 移到 tick 0（与 id0 同 tick）
    assert!(list.restore_sorted(&[2]));
    let ids: Vec<u32> = list.iter().map(|e| e.id).collect();
    assert_eq!(
        ids,
        vec![0, 2, 1],
        "同 tick 时未移动的 id0 应在移动的 id2 之前"
    );
    assert_eq!(ticks(&list), vec![0, 0, 10]);
}

#[test]
fn test_restore_sorted_across_chunks() {
    // 跨块：3 块 × 3 元素（tick 0..8）；把末块首元素（tick 6）搬到最前
    let mut list = multi_chunk(&[3, 3, 3]);
    set_tick(&mut list, 6, 1); // 0,1,2,3,4,5,1,7,8
    assert!(list.restore_sorted(&[6]));
    assert!(is_sorted(&list));
    assert_eq!(ticks(&list), vec![0, 1, 1, 2, 3, 4, 5, 7, 8]);
}

#[test]
fn test_restore_sorted_empty_and_out_of_range_moved() {
    let mut list = ChunkedList::from_sorted(sorted_events(3));
    assert!(!list.restore_sorted(&[]), "空 moved 为空操作");
    assert!(!list.restore_sorted(&[99]), "越界 moved 被剔除后为空操作");
    assert_eq!(ticks(&list), vec![0, 10, 20]);
}

#[test]
fn test_restore_sorted_vec_matches_list_semantics() {
    let mut v = sorted_events(4); // 0,10,20,30
    v[0].tick = 25; // 25,10,20,30
    let (restored, ranges) = restore_sorted_vec(&v, &[0]).expect("失序应重排");
    assert_eq!(
        restored.iter().map(|e| e.tick).collect::<Vec<_>>(),
        vec![10, 20, 25, 30]
    );
    assert_eq!(ranges, vec![(0, 2)], "旧位置 0 → 新位置 2，区间应包住");
    let sorted = sorted_events(4);
    assert!(
        restore_sorted_vec(&sorted, &[1]).is_none(),
        "仍有序应返回 None（零重排）"
    );
}

#[test]
fn test_sorted_locally_in_slice_detects_inversion() {
    let mut v = sorted_events(3);
    assert!(sorted_locally_in_slice(&v, &[0, 1, 2]));
    v[0].tick = 100; // 100,10,20 → 逆序
    assert!(!sorted_locally_in_slice(&v, &[0]));
    // 局部检查契约：漏传被修改索引可能漏检（调用方必须传全）
    let v2 = sorted_events(3);
    assert!(sorted_locally_in_slice(&v2, &[1]));
}

// ── 受影响区间集合（restore_sorted_ranges）：区间增量更新的正确性基础 ──

#[test]
fn test_restore_sorted_ranges_tight_single_move() {
    // 0..90；index 5（tick 50）→ 95 → 落位 index 9
    let mut list = ChunkedList::from_sorted(sorted_events(10));
    set_tick(&mut list, 5, 95);
    let ranges = list.restore_sorted_ranges(&[5]).expect("失序应重排");
    assert_eq!(ranges, vec![(5, 9)], "区间 = 旧位置 ∪ 新位置（紧致）");
    assert!(is_sorted(&list));
}

#[test]
fn test_restore_sorted_ranges_covers_all_shifts() {
    // 0..90；index 2(20)→45、index 4(40)→55：中间 3/5 均被位移
    let mut list = ChunkedList::from_sorted(sorted_events(10));
    set_tick(&mut list, 2, 45);
    set_tick(&mut list, 4, 55);
    let ranges = list.restore_sorted_ranges(&[2, 4]).expect("失序应重排");
    assert_eq!(
        ranges,
        vec![(2, 5)],
        "近邻区间合并后必须覆盖被位移元素（[45,50,55,60] 对比 [20,30,40,50]）"
    );
    assert!(is_sorted(&list));
}

#[test]
fn test_restore_sorted_ranges_tie_block_conservative() {
    // index 2（tick 20）→ 0（与 index 0 同 tick）：同 tick 块整体纳入区间
    let mut list = ChunkedList::from_sorted(sorted_events(4));
    set_tick(&mut list, 2, 0);
    let ranges = list.restore_sorted_ranges(&[2]).expect("失序应重排");
    assert_eq!(
        ranges,
        vec![(0, 2)],
        "同 tick 块保守覆盖（宁可多传，不可漏）"
    );
}

#[test]
fn test_restore_sorted_ranges_none_when_sorted() {
    let mut list = ChunkedList::from_sorted(sorted_events(4));
    set_tick(&mut list, 2, 25); // 仍有序
    assert!(
        list.restore_sorted_ranges(&[2]).is_none(),
        "零重排返回 None"
    );
}

#[test]
fn test_restore_sorted_ranges_scattered_stays_sparse() {
    // 200 音符；每隔 20 个选一个（0,20,...,180）各自 +15 tick（越过下一个邻居）
    // → 受影响区间必须**稀疏**（不得退化为横跨整轨的凸包）
    let mut list = ChunkedList::from_sorted(sorted_events(200)); // ticks 0..1990
    let moved: Vec<usize> = (0..200).step_by(20).collect();
    let before: Vec<u32> = list.iter().map(|e| e.id).collect();
    for &i in &moved {
        set_tick(&mut list, i, i as u32 * 10 + 15);
    }
    let ranges = list.restore_sorted_ranges(&moved).expect("失序应重排");
    assert!(is_sorted(&list));
    let payload: usize = ranges.iter().map(|&(lo, hi)| hi - lo + 1).sum();
    let hull = ranges.last().expect("非空").1 - ranges[0].0 + 1;
    assert!(
        payload * 4 < hull,
        "分散改动负载必须显著小于凸包（实际 {payload} vs 凸包 {hull}）"
    );
    assert!(ranges.len() >= 5, "分散改动应产生多个稀疏区间: {ranges:?}");
    // 契约：区间外内容逐位一致
    let after: Vec<u32> = list.iter().map(|e| e.id).collect();
    let covered = |i: usize| ranges.iter().any(|&(lo, hi)| i >= lo && i <= hi);
    for i in 0..before.len() {
        if !covered(i) {
            assert_eq!(before[i], after[i], "区间外索引 {i} 内容必须不变");
        }
    }
}

#[test]
fn test_restore_sorted_span_large_path_merge() {
    // 40 元素；index 10..27（17 个，> 阈值 16）搬到末尾 → 归并路径
    let mut list = ChunkedList::from_sorted(sorted_events(40));
    for i in 10..27 {
        set_tick(&mut list, i, 1000 + i as u32);
    }
    let moved: Vec<usize> = (10..27).collect();
    let ranges = list.restore_sorted_ranges(&moved).expect("失序应重排");
    assert!(is_sorted(&list));
    assert_eq!(
        ranges,
        vec![(10, 39)],
        "区间 = 旧位置(10..26) ∪ 新位置(23..39)（前 10 个不变）"
    );
}

#[test]
fn test_restore_sorted_ranges_interval_content_contract() {
    // 契约（区间更新完备性核心）：区间外内容与重排前**逐位一致**
    for &(idx, new_tick) in &[(5usize, 95u32), (5, 5), (20, 25), (0, 235), (23, 3)] {
        let n = 24usize;
        let mut list = ChunkedList::from_sorted(sorted_events(n));
        let before: Vec<u32> = list.iter().map(|e| e.id).collect();
        set_tick(&mut list, idx, new_tick);
        let ranges = list
            .restore_sorted_ranges(&[idx])
            .expect("该场景必须失序重排");
        let after: Vec<u32> = list.iter().map(|e| e.id).collect();
        for i in 0..before.len() {
            let covered = ranges.iter().any(|&(lo, hi)| i >= lo && i <= hi);
            if !covered {
                assert_eq!(
                    before[i], after[i],
                    "idx={idx}: 区间外索引 {i} 内容必须不变（ranges={ranges:?}）"
                );
            }
        }
        assert!(is_sorted(&list));
    }
}
