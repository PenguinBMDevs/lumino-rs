//! 构建 / 插入 / 定位 / 分裂测试

use super::util::{TestEvent, make_test_event, reference_insert, sorted_events};
use crate::chunked_list::{ChunkedList, EVENT_CHUNK_CAPACITY, EVENT_CHUNK_SPLIT, EventTick};

#[test]
fn test_from_sorted_basic() {
    let list = ChunkedList::from_sorted(sorted_events(10));
    assert_eq!(list.len(), 10);
    assert_eq!(list.first().expect("列表首元素应存在").tick, 0);
    assert_eq!(list.last().expect("列表末元素应存在").tick, 90);
    assert_eq!(list.get(5).expect("索引 5 的事件应存在").tick, 50);
    assert_eq!(list.get(9).expect("索引 9 的事件应存在").tick, 90);
    assert_eq!(list.get(10), None);
}

#[test]
fn test_from_sorted_iter_basic() {
    // 与 from_sorted 等价的迭代器构建（零中间 Vec）
    let list = ChunkedList::from_sorted_iter(sorted_events(10));
    assert_eq!(list.len(), 10);
    assert_eq!(list.first().expect("列表首元素应存在").tick, 0);
    assert_eq!(list.last().expect("列表末元素应存在").tick, 90);
    assert_eq!(list.get(5).expect("索引 5 的事件应存在").tick, 50);
    assert_eq!(list.get(10), None);
    // 与 from_sorted 构建内容完全一致
    assert_eq!(list.to_vec(), sorted_events(10));
}

#[test]
fn test_from_sorted_iter_empty() {
    let list = ChunkedList::from_sorted_iter(std::iter::empty::<TestEvent>());
    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
    assert_eq!(list.chunk_count(), 0);
}

#[test]
fn test_from_sorted_iter_chunk_boundaries() {
    // 70 万事件 = 2 块（50 万 + 20 万），验证跨块一致性
    let list = ChunkedList::from_sorted_iter(sorted_events(700_000));
    assert_eq!(list.len(), 700_000);
    assert_eq!(list.chunk_count(), 2);
    assert_eq!(list.get(0).expect("首元素应存在").tick, 0);
    assert_eq!(
        list.get(499_999).expect("块 0 末元素应存在").tick,
        4_999_990
    );
    assert_eq!(
        list.get(500_000).expect("块 1 首元素应存在").tick,
        5_000_000
    );
    assert_eq!(list.get(699_999).expect("末元素应存在").tick, 6_999_990);
    // 与 from_sorted 构建内容完全一致（含跨块）
    assert_eq!(list.to_vec(), sorted_events(700_000));
}

#[test]
fn test_empty_list() {
    let list: ChunkedList<TestEvent> = ChunkedList::new();
    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
    assert_eq!(list.get(0), None);
    assert_eq!(list.partition_point(100), 0);
    assert_eq!(list.iter().count(), 0);
}

#[test]
fn test_insert_into_empty() {
    let mut list: ChunkedList<TestEvent> = ChunkedList::new();
    list.insert(make_test_event(50, 0));
    assert_eq!(list.len(), 1);
    assert_eq!(list.first().expect("列表首元素应存在").tick, 50);
}

#[test]
fn test_insert_middle_preserves_order() {
    let mut list = ChunkedList::from_sorted(sorted_events(10));
    // 插到 tick=50 之前（50 前插入 → 稳定插到同 tick 后）
    list.insert(make_test_event(45, 99));
    let ticks: Vec<u32> = list.iter().map(EventTick::tick).collect();
    assert_eq!(ticks, vec![0, 10, 20, 30, 40, 45, 50, 60, 70, 80, 90]);
    // 同 tick 稳定插入
    list.insert(make_test_event(45, 100));
    let ticks: Vec<u32> = list.iter().map(EventTick::tick).collect();
    assert_eq!(ticks, vec![0, 10, 20, 30, 40, 45, 45, 50, 60, 70, 80, 90]);
    assert_eq!(list.len(), 12);
}

#[test]
fn test_position_of_finds_exact_value() {
    let mut list = ChunkedList::from_sorted(sorted_events(10));
    // 同 tick 多事件：按值精确匹配（id 区分）
    list.insert(make_test_event(50, 100));
    list.insert(make_test_event(50, 101));
    assert_eq!(
        list.position_of(&make_test_event(50, 100)),
        Some(6),
        "同 tick 事件插到 id=5 之后"
    );
    assert_eq!(list.position_of(&make_test_event(50, 101)), Some(7));
    assert_eq!(
        list.position_of(&make_test_event(50, 5)),
        Some(5),
        "原始 id=5 事件仍在 index 5"
    );
    assert_eq!(list.position_of(&make_test_event(20, 2)), Some(2));
    assert_eq!(
        list.position_of(&make_test_event(90, 9)),
        Some(11),
        "尾部事件 index 顺延 2"
    );

    // 不存在的值 → None
    assert_eq!(list.position_of(&make_test_event(55, 999)), None);
    assert_eq!(
        list.position_of(&make_test_event(50, 999)),
        None,
        "同 tick 但 id 不匹配"
    );
}

#[test]
fn test_position_of_empty_and_removal_roundtrip() {
    let mut list: ChunkedList<_> = ChunkedList::new();
    assert_eq!(list.position_of(&make_test_event(0, 0)), None);

    list.insert(make_test_event(10, 1));
    list.insert(make_test_event(40, 4));
    list.insert(make_test_event(20, 2));
    assert_eq!(list.position_of(&make_test_event(20, 2)), Some(1));

    // 删除后定位正确
    let idx = list
        .position_of(&make_test_event(20, 2))
        .expect("应定位到目标事件");
    let removed = list.remove(idx).expect("目标事件应存在");
    assert_eq!(removed, make_test_event(20, 2));
    assert_eq!(list.position_of(&make_test_event(20, 2)), None);
    assert_eq!(list.position_of(&make_test_event(40, 4)), Some(1));
}

#[test]
fn test_position_of_across_chunk_boundary() {
    // 用小块容量强制跨块：EVENT_CHUNK_CAPACITY 是 const，这里用手工多事件验证
    let mut list = ChunkedList::from_sorted(sorted_events(60));
    for i in 60..70u32 {
        list.insert(make_test_event(i * 10, i));
    }
    // 找尾部事件（跨块续扫路径）
    assert_eq!(list.position_of(&make_test_event(690, 69)), Some(69));
    assert_eq!(list.position_of(&make_test_event(300, 30)), Some(30));
    assert_eq!(list.position_of(&make_test_event(0, 0)), Some(0));
}

#[test]
fn test_insert_before_first_and_after_last() {
    let mut list = ChunkedList::from_sorted(sorted_events(10));
    list.insert(make_test_event(5, 98)); // 首元素（tick=0）之后、第二元素之前
    list.insert(make_test_event(95, 97)); // 末元素（tick=90）之后
    let ticks: Vec<u32> = list.iter().map(EventTick::tick).collect();
    assert_eq!(ticks, vec![0, 5, 10, 20, 30, 40, 50, 60, 70, 80, 90, 95]);
    assert_eq!(list.len(), 12);
}

#[test]
fn test_partition_point_matches_reference() {
    let mut list = ChunkedList::from_sorted(sorted_events(1000));
    let mut reference: Vec<_> = sorted_events(1000);

    // 随机插入 200 个事件，对照 partition_point
    let mut seed: u64 = 42;
    for i in 0..200 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let tick = (seed % 10_000) as u32;
        let e = make_test_event(tick, i);
        list.insert(e);
        reference_insert(&mut reference, e);
    }

    assert_eq!(list.len(), reference.len());
    // 全局顺序一致
    let list_ticks: Vec<u32> = list.iter().map(EventTick::tick).collect();
    let ref_ticks: Vec<u32> = reference.iter().map(|e| e.tick).collect();
    assert_eq!(list_ticks, ref_ticks);

    // partition_point 一致
    for probe in [0u32, 5, 49, 50, 51, 999, 10_000, 20_000] {
        assert_eq!(
            list.partition_point(probe),
            reference.partition_point(|e| e.tick < probe),
            "partition_point mismatch at {probe}"
        );
    }
}

#[test]
fn test_split_on_full_chunk() {
    // 构造 50 万满块
    let mut list = ChunkedList::from_sorted(sorted_events(EVENT_CHUNK_CAPACITY));
    assert_eq!(list.chunk_count(), 1);
    assert_eq!(list.len(), EVENT_CHUNK_CAPACITY);

    // 向满块中间插入 → 触发分裂
    list.insert(make_test_event(
        EVENT_CHUNK_CAPACITY as u32 * 10 / 2,
        999_999,
    ));
    assert_eq!(list.chunk_count(), 2, "满块插入应分裂为 2 块");
    assert_eq!(list.len(), EVENT_CHUNK_CAPACITY + 1);

    // 顺序保持
    let ticks: Vec<u32> = list.iter().map(EventTick::tick).collect();
    assert!(ticks.windows(2).all(|w| w[0] <= w[1]), "分裂后必须保持升序");

    // 每块事件数 ≤ 容量
    assert!(list.chunks.iter().all(|c| c.len() <= EVENT_CHUNK_CAPACITY));
    // get 跨块索引正确
    assert_eq!(
        list.get(EVENT_CHUNK_SPLIT).expect("分裂点事件应存在").tick,
        (EVENT_CHUNK_SPLIT as u32) * 10
    );
}

#[test]
fn test_repeated_splits_stay_consistent() {
    // 连续插入触发多次分裂：50 万 + 10 万随机
    let mut list = ChunkedList::from_sorted(sorted_events(EVENT_CHUNK_CAPACITY));
    let mut reference: Vec<_> = sorted_events(EVENT_CHUNK_CAPACITY);

    let mut seed: u64 = 7;
    for i in 0..100_000 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let tick = (seed % (EVENT_CHUNK_CAPACITY as u64 * 2)) as u32;
        let e = make_test_event(tick, i);
        list.insert(e);
        reference_insert(&mut reference, e);
    }

    assert_eq!(list.len(), reference.len());
    assert!(list.chunks.iter().all(|c| c.len() <= EVENT_CHUNK_CAPACITY));
    let list_ticks: Vec<u32> = list.iter().map(EventTick::tick).collect();
    let ref_ticks: Vec<u32> = reference.iter().map(|e| e.tick).collect();
    assert_eq!(list_ticks, ref_ticks, "多次分裂后顺序必须与参照一致");
}

#[test]
fn test_large_insert_performance_shape() {
    // 验证 50 万规模插入仍是块内操作（不崩、顺序正确）
    let mut list = ChunkedList::from_sorted(sorted_events(EVENT_CHUNK_CAPACITY));
    list.insert(make_test_event(3, 1));
    list.insert(make_test_event(3, 2));
    assert_eq!(list.len(), EVENT_CHUNK_CAPACITY + 2);
    assert_eq!(list.first().expect("列表首元素应存在").tick, 0);
    assert_eq!(list.get(2).expect("索引 2 的事件应存在").tick, 3);
}
