//! 删除 / 范围查询 / 替换与清空 / 转回 Vec 测试

use super::util::{make_test_event, sorted_events};
use crate::chunked_list::{ChunkedList, EventTick};

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
