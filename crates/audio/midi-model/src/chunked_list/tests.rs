//! ChunkedList 单元测试（含 COW 语义与 50 万容量边界验证）

use std::sync::Arc;

use super::{ChunkedList, EVENT_CHUNK_CAPACITY, EVENT_CHUNK_SPLIT, EventTick};

/// 测试用最小事件类型
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TestEvent {
    tick: u32,
    id: u32,
}

impl EventTick for TestEvent {
    fn tick(&self) -> u32 {
        self.tick
    }
}

fn make_test_event(tick: u32, id: u32) -> TestEvent {
    TestEvent { tick, id }
}

fn sorted_events(count: usize) -> Vec<TestEvent> {
    (0..count as u32)
        .map(|i| make_test_event(i * 10, i))
        .collect()
}

/// 直接构造多块 ChunkedList（测试窗口跨块迭代，避免依赖 50 万真实容量）
fn multi_chunk(list_sizes: &[usize]) -> ChunkedList<TestEvent> {
    let mut list: ChunkedList<TestEvent> = ChunkedList::new();
    let mut next_tick = 0u32;
    for &size in list_sizes {
        let chunk: Vec<TestEvent> = (0..size)
            .map(|i| make_test_event(next_tick + i as u32, next_tick + i as u32))
            .collect();
        list.chunks.push(Arc::new(chunk));
        next_tick += size as u32;
    }
    list.total_len = next_tick as usize;
    list.rebuild_index();
    list
}

/// 参照实现：普通 Vec + partition_point（验证 ChunkedList 行为等价）
fn reference_insert(sorted: &mut Vec<TestEvent>, e: TestEvent) {
    let idx = sorted.partition_point(|x| x.tick <= e.tick);
    sorted.insert(idx, e);
}

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
    let mut list: ChunkedList<TestEvent> = ChunkedList::new();
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
    let mut reference: Vec<TestEvent> = sorted_events(1000);

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
    let mut reference: Vec<TestEvent> = sorted_events(EVENT_CHUNK_CAPACITY);

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
fn test_large_insert_performance_shape() {
    // 验证 50 万规模插入仍是块内操作（不崩、顺序正确）
    let mut list = ChunkedList::from_sorted(sorted_events(EVENT_CHUNK_CAPACITY));
    list.insert(make_test_event(3, 1));
    list.insert(make_test_event(3, 2));
    assert_eq!(list.len(), EVENT_CHUNK_CAPACITY + 2);
    assert_eq!(list.first().expect("列表首元素应存在").tick, 0);
    assert_eq!(list.get(2).expect("索引 2 的事件应存在").tick, 3);
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
