//! `window_range` / `iter_window` 窗口定位与跨块迭代测试

use super::util::TestEvent;
use super::util::{multi_chunk, sorted_events};
use crate::chunked_list::ChunkedList;

// ── window_range：lookback 窗口定位 ─────────────────────────

#[test]
fn test_window_range_basic() {
    let list = ChunkedList::from_sorted(sorted_events(10));
    // event ticks: 0,10,..,90；partition_point(t) = tick<t 的事件数
    // [30, 60) 无 lookback → 全局索引 [3, 6)（事件 30,40,50）
    assert_eq!(list.window_range(30, 60, 0), (3, 6));
    // lookback=15 → 起点 tick=15 → 分区 2（事件 0,10）→ [2, 6)
    assert_eq!(list.window_range(30, 60, 15), (2, 6));
    // lookback 超首 → 起点 0
    assert_eq!(list.window_range(0, 30, u32::MAX), (0, 3));
    // end < start → (pp(end), pp(start)) 即 lo>hi，由调用方保证不越界
    assert_eq!(list.window_range(60, 30, 0), (6, 3));
}

// ── iter_window：跨块惰性迭代 ───────────────────────────────

#[test]
fn test_iter_window_single_chunk() {
    let list = ChunkedList::from_sorted(sorted_events(10));
    let got: Vec<(usize, u32)> = list.iter_window(2, 5).map(|(i, e)| (i, e.tick)).collect();
    assert_eq!(got, vec![(2, 20), (3, 30), (4, 40)]);
}

#[test]
fn test_iter_window_cross_chunks() {
    let list = multi_chunk(&[3, 4, 3]); // tick: 0..30 → 三块 [0,3) [3,7) [7,10)
    // 跨两块：从块 0 采样 [2, 5) → 块 0 的 index 2 和块 1 的 index 3,4
    let got: Vec<(usize, u32)> = list.iter_window(2, 5).map(|(i, e)| (i, e.tick)).collect();
    assert_eq!(got, vec![(2, 2), (3, 3), (4, 4)]);
    // 全跨三块
    let got: Vec<(usize, u32)> = list.iter_window(0, 10).map(|(i, e)| (i, e.tick)).collect();
    assert_eq!(got.len(), 10);
    assert_eq!(got.first().expect("首元素应存在"), &(0, 0));
    assert_eq!(got.last().expect("末元素应存在"), &(9, 9));
}

#[test]
fn test_iter_window_bounds_and_empty() {
    let list = ChunkedList::from_sorted(sorted_events(10));
    // 越界 clamp：hi 超 len
    let got: Vec<(usize, u32)> = list.iter_window(8, 999).map(|(i, e)| (i, e.tick)).collect();
    assert_eq!(got, vec![(8, 80), (9, 90)]);
    // lo > len → 空
    assert_eq!(list.iter_window(10, 20).count(), 0);
    // 空窗口
    assert_eq!(list.iter_window(5, 5).count(), 0);
    // 空容器
    let empty: ChunkedList<TestEvent> = ChunkedList::new();
    assert_eq!(empty.iter_window(0, 10).count(), 0);
}

#[test]
fn test_iter_window_size_hint() {
    let list = multi_chunk(&[3, 4, 3]);
    let mut it = list.iter_window(2, 7);
    assert_eq!(it.size_hint(), (5, Some(5)));
    it.next();
    assert_eq!(it.size_hint(), (4, Some(4)));
}
