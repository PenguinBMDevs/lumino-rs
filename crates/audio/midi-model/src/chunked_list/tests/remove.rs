//! 删除 / 范围查询 / 替换与清空 / 转回 Vec 测试

use super::util::{make_test_event, multi_chunk, sorted_events};
use crate::chunked_list::{ChunkedList, EventTick};

/// 参照实现：按降序区间逐个删除（与 `remove_ranges` 语义对齐）
fn reference_remove_ranges(ticks: &mut Vec<u32>, ranges: &[(usize, usize)]) {
    for &(start, count) in ranges {
        for _ in 0..count {
            if start < ticks.len() {
                ticks.remove(start);
            }
        }
    }
}

#[test]
fn test_remove_ranges_matches_reference() {
    let sizes = [4usize, 4, 4];
    let cases: Vec<Vec<(usize, usize)>> = vec![
        vec![(8, 4)],                         // 尾块整块
        vec![(3, 6)],                         // 跨块连续
        vec![(9, 1), (7, 1), (5, 1), (3, 1)], // 稀疏
        vec![(0, 12)],                        // 全量
        vec![(11, 1), (0, 1)],                // 首尾各一
        vec![(10, 100)],                      // 尾部越界截断
        vec![(2, 1), (1, 1), (0, 1)],         // 相邻未合并区间（防御：等价于 (0,3)）
    ];
    for ranges in cases {
        let mut list = multi_chunk(&sizes);
        let mut ref_ticks: Vec<u32> = (0..12).collect();
        reference_remove_ranges(&mut ref_ticks, &ranges);
        let removed = list.remove_ranges(&ranges);
        assert_eq!(list.len(), ref_ticks.len(), "ranges={ranges:?}");
        assert_eq!(removed, 12 - ref_ticks.len(), "ranges={ranges:?}");
        let got: Vec<u32> = list.iter().map(|e| e.tick).collect();
        assert_eq!(got, ref_ticks, "ranges={ranges:?}");
        // 索引与块偏移一致性
        for (i, t) in ref_ticks.iter().enumerate() {
            assert_eq!(
                list.get(i).map(|e| e.tick),
                Some(*t),
                "ranges={ranges:?} i={i}"
            );
        }
    }
}

#[test]
fn test_remove_ranges_full_drops_all_chunks() {
    let mut list = multi_chunk(&[4, 4, 4]);
    let removed = list.remove_ranges(&[(0, 12)]);
    assert_eq!(removed, 12);
    assert!(list.is_empty());
    assert_eq!(list.chunk_count(), 0);
    // 清空后仍可插入（空容器路径）
    list.insert(make_test_event(1, 1));
    assert_eq!(list.len(), 1);
}

#[test]
fn test_remove_ranges_empty_or_oob_is_noop() {
    let mut list = multi_chunk(&[4, 4, 4]);
    assert_eq!(list.remove_ranges(&[]), 0);
    assert_eq!(list.remove_ranges(&[(100, 5)]), 0);
    assert_eq!(list.len(), 12);
}

#[test]
fn test_remove_ranges_cow_preserves_clone() {
    let mut list = multi_chunk(&[4, 4, 4]);
    let snapshot = list.clone();
    let removed = list.remove_ranges(&[(0, 6)]);
    assert_eq!(removed, 6);
    assert_eq!(list.len(), 6);
    assert_eq!(snapshot.len(), 12);
    let snap_ticks: Vec<u32> = snapshot.iter().map(|e| e.tick).collect();
    assert_eq!(snap_ticks, (0..12).collect::<Vec<u32>>());
    let got: Vec<u32> = list.iter().map(|e| e.tick).collect();
    assert_eq!(got, vec![6, 7, 8, 9, 10, 11]);
}

#[test]
fn test_remove_and_remove_by_tick() {
    let mut list = ChunkedList::from_sorted(sorted_events(1000));
    // 移除中间
    let removed = list.remove(500).expect("索引 500 的事件应存在");
    assert_eq!(removed.tick, 5000);
    assert_eq!(list.len(), 999);
    assert_eq!(list.get(499).expect("索引 499 的事件应存在").tick, 4990);
    assert_eq!(list.get(500).expect("索引 500 的事件应存在").tick, 5010);

    // 按 tick 删除
    let removed = list.remove_by_tick(5010).expect("tick 5010 的事件应存在");
    assert_eq!(removed.tick, 5010);
    assert_eq!(list.len(), 998);

    // 删除不存在
    assert!(list.remove_by_tick(12345).is_none());

    // 越界
    assert!(list.remove(9999).is_none());
}

#[test]
fn test_range_query() {
    let mut list = ChunkedList::from_sorted(sorted_events(1000));
    // 混入不同 tick
    list.insert(make_test_event(123, 1));
    list.insert(make_test_event(456, 2));

    let range: Vec<u32> = list.range(100, 500).map(EventTick::tick).collect();
    assert_eq!(
        range,
        vec![
            100, 110, 120, 123, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240, 250,
            260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390, 400, 410, 420,
            430, 440, 450, 456, 460, 470, 480, 490
        ]
    );
}

#[test]
fn test_replace_and_clear() {
    let mut list = ChunkedList::from_sorted(sorted_events(100));
    list.replace_sorted(sorted_events(200));
    assert_eq!(list.len(), 200);
    list.clear();
    assert!(list.is_empty());
    assert_eq!(list.chunk_count(), 0);
    list.insert(make_test_event(1, 1));
    assert_eq!(list.len(), 1);
}

#[test]
fn test_to_vec_roundtrip() {
    let list = ChunkedList::from_sorted(sorted_events(700_000));
    assert_eq!(list.chunk_count(), 2, "70 万事件应分 2 块");
    let back = list.to_vec();
    assert_eq!(back.len(), 700_000);
    assert_eq!(back[0].tick, 0);
    assert_eq!(back[699_999].tick, 6_999_990);
}
