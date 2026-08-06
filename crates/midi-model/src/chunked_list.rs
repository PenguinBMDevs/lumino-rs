//! ChunkedList — 泛型分块有序事件容器
//!
//! 2026-08-06 阶段二：解决超大单一 Vec 插入阻塞（O(N) memmove）。
//! 按 tick 有序的事件序列（音符 / CC/PC/PB 控制事件）统一使用本容器：
//! 每块最多 50 万事件，满块插入时动态分裂为两个 25 万块。
//!
//! 设计要点：
//! - 块间与块内均按 tick 升序，块级二分（`chunk_first_ticks`）+ 块内二分（`partition_point`）
//! - `insert` 只移动目标块内元素（≤ 25 万），跨块 O(log 块数) 定位
//! - `partition_point` 为真二分（块级 + 块内），播放引擎 seek 热路径依赖
//! - 泛型 T 只需实现 `EventTick`（提供 `tick()`）

/// 单块容量：50 万事件
pub const EVENT_CHUNK_CAPACITY: usize = 500_000;
/// 分裂点：满块分裂为两个 25 万块（容量保持 50 万不变）
pub const EVENT_CHUNK_SPLIT: usize = 250_000;

/// 有序事件特征：容器元素必须能取出 tick（排序/二分键）
pub trait EventTick {
    fn tick(&self) -> u32;
}

/// `midly::loader::PackedControlEvent` 的有序事件实现（CC/PC/PB 控制事件）
impl EventTick for midly::loader::PackedControlEvent {
    #[inline]
    fn tick(&self) -> u32 {
        self.tick
    }
}

/// 泛型分块有序事件容器
#[derive(Clone, Debug)]
pub struct ChunkedList<T> {
    /// 分块，块间按首事件 tick 升序，块内按 tick 升序
    chunks: Vec<Vec<T>>,
    /// 每块首事件 tick（块级二分索引）
    chunk_first_ticks: Vec<u32>,
    /// 前缀和：chunk_offsets[i] = 前 i 块的事件总数
    chunk_offsets: Vec<usize>,
    /// 总事件数
    total_len: usize,
}

impl<T: EventTick> ChunkedList<T> {
    /// 从已按 tick 升序排列的事件构建分块容器（O(N)）。
    ///
    /// 注意：调用方需保证 `events` 已按 tick 升序。构建时按容量切块，
    /// 不会做排序（与 MidiDocument 构建路径的 `sort_unstable` 配合）。
    pub fn from_sorted(events: Vec<T>) -> Self {
        if events.is_empty() {
            return Self::new();
        }
        let mut chunks: Vec<Vec<T>> =
            Vec::with_capacity(events.len().div_ceil(EVENT_CHUNK_CAPACITY));
        let mut chunk_first_ticks = Vec::with_capacity(chunks.capacity());
        let mut chunk_offsets = Vec::with_capacity(chunks.capacity() + 1);
        chunk_offsets.push(0);

        let mut total_len = 0usize;
        let mut iter = events.into_iter();
        loop {
            let chunk: Vec<T> = iter.by_ref().take(EVENT_CHUNK_CAPACITY).collect();
            if chunk.is_empty() {
                break;
            }
            let first_tick = chunk.first().map(EventTick::tick).unwrap_or(0);
            let chunk_len = chunk.len();
            chunks.push(chunk);
            chunk_first_ticks.push(first_tick);
            total_len += chunk_len;
            chunk_offsets.push(total_len);
        }

        Self {
            chunks,
            chunk_first_ticks,
            chunk_offsets,
            total_len,
        }
    }

    /// 空容器
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            chunk_first_ticks: Vec::new(),
            chunk_offsets: vec![0],
            total_len: 0,
        }
    }

    /// 静态空实例（返回 `&ChunkedList` 的默认空引用，避免分配）
    pub const EMPTY: Self = Self {
        chunks: Vec::new(),
        chunk_first_ticks: Vec::new(),
        chunk_offsets: Vec::new(),
        total_len: 0,
    };

    /// 空容器（带容量预分配，供批量构建场景使用）
    pub fn with_capacity(capacity: usize) -> Self {
        let chunk_count = capacity.div_ceil(EVENT_CHUNK_CAPACITY);
        Self {
            chunks: Vec::with_capacity(chunk_count),
            chunk_first_ticks: Vec::with_capacity(chunk_count),
            chunk_offsets: vec![0],
            total_len: 0,
        }
    }

    /// 事件总数（O(1)）
    #[inline]
    pub fn len(&self) -> usize {
        self.total_len
    }

    /// 是否为空（O(1)）
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.total_len == 0
    }

    /// 块数量（调试/监控用）
    #[inline]
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// 已分配的事件容量（监控用，近似旧 `Vec::capacity` 语义）
    ///
    /// 返回所有块已分配容量之和；快满时会略高于 `len()`（预留增长空间）。
    #[inline]
    pub fn capacity(&self) -> usize {
        self.chunks.iter().map(Vec::capacity).sum()
    }

    /// 全局索引访问（O(log 块数)）
    ///
    /// 二分定位所在块后取块内索引。越界返回 None。
    #[inline]
    pub fn get(&self, index: usize) -> Option<&T> {
        if index >= self.total_len {
            return None;
        }
        // 在 chunk_offsets 中二分：最后一个 offset <= index 的块
        let ci = self.chunk_offsets.partition_point(|&o| o <= index) - 1;
        let local = index - self.chunk_offsets[ci];
        self.chunks[ci].get(local)
    }

    /// 全局索引可变访问（O(log 块数)）
    ///
    /// 返回目标块内的可变引用。越界返回 None。
    /// 注意：修改事件 tick 会破坏排序不变式，调用方需自行保证（与旧 `&mut Vec` 语义一致）。
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        if index >= self.total_len {
            return None;
        }
        let ci = self.chunk_offsets.partition_point(|&o| o <= index) - 1;
        let local = index - self.chunk_offsets[ci];
        self.chunks[ci].get_mut(local)
    }

    /// 首事件（O(1)）
    #[inline]
    pub fn first(&self) -> Option<&T> {
        self.chunks.first().and_then(|c| c.first())
    }

    /// 末事件（O(1)）
    #[inline]
    pub fn last(&self) -> Option<&T> {
        self.chunks.last().and_then(|c| c.last())
    }

    /// 跨块迭代器（按 tick 升序）
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.chunks.iter().flat_map(|c| c.iter())
    }

    /// 追加事件到末尾（O(1) 摊还）
    ///
    /// 调用方需保证 `event.tick >= 当前末事件 tick`（升序追加语义，
    /// 与旧 `Vec::push` 在有序轨道上的用法一致）。不满足时行为等价于
    /// `insert`（自动定位正确位置，但代价更高）。
    pub fn push_back(&mut self, event: T) {
        // 快速路径：末尾块存在、非空、未满、且事件 tick 不早于末事件
        if let Some(last) = self.chunks.last_mut()
            && let Some(last_ev) = last.last()
            && last_ev.tick() <= event.tick()
            && last.len() < EVENT_CHUNK_CAPACITY
        {
            last.push(event);
            self.total_len += 1;
            // 更新末尾前缀和（chunk_offsets 末项 = 更新后的总数）
            if let Some(last_offset) = self.chunk_offsets.last_mut() {
                *last_offset = self.total_len;
            }
            return;
        }
        // 兜底：空容器 / 空块 / 末尾块满 / 乱序 → 走有序插入（内部处理分裂与索引）
        self.insert(event);
    }

    /// 按 tick 升序插入事件（O(块内) + O(log 块数)）
    ///
    /// 定位目标块后块内二分插入；若目标块已满（50 万），先分裂为两个
    /// 25 万块，再插入目标半块。
    pub fn insert(&mut self, event: T) {
        let tick = event.tick();
        if self.chunks.is_empty() {
            let mut chunk = Vec::with_capacity(EVENT_CHUNK_CAPACITY);
            chunk.push(event);
            self.chunks.push(chunk);
            self.chunk_first_ticks.push(tick);
            self.chunk_offsets = vec![0, 1];
            self.total_len = 1;
            return;
        }

        let ci = self.locate_chunk(tick);
        let chunk = &mut self.chunks[ci];

        if chunk.len() >= EVENT_CHUNK_CAPACITY {
            // 满块分裂：切成两个 25 万块，插入目标半块
            self.split_chunk(ci);
            let ci = self.locate_chunk(tick);
            let chunk = &mut self.chunks[ci];
            let local = chunk.partition_point(|e| e.tick() <= tick);
            chunk.insert(local, event);
        } else {
            let local = chunk.partition_point(|e| e.tick() <= tick);
            chunk.insert(local, event);
        }

        self.total_len += 1;
        self.rebuild_index();
    }

    /// 整轨替换（O(N) 重建）
    ///
    /// `events` 需按 tick 升序（调用方负责排序，与旧 `Vec` 赋值语义一致）。
    pub fn replace_sorted(&mut self, events: Vec<T>) {
        *self = Self::from_sorted(events);
    }

    /// 清空（O(块数)）
    pub fn clear(&mut self) {
        self.chunks.clear();
        self.chunk_first_ticks.clear();
        self.chunk_offsets = vec![0];
        self.total_len = 0;
    }

    /// 移除全局索引处的事件，返回被移除事件的副本（O(块内)）
    pub fn remove(&mut self, index: usize) -> Option<T>
    where
        T: Clone,
    {
        if index >= self.total_len {
            return None;
        }
        let ci = self.chunk_offsets.partition_point(|&o| o <= index) - 1;
        let local = index - self.chunk_offsets[ci];
        let removed = self.chunks[ci].remove(local);
        self.total_len -= 1;
        // 空块清理（保留至少一个块用于索引一致性）
        if self.chunks[ci].is_empty() && self.chunks.len() > 1 {
            self.chunks.remove(ci);
        }
        self.rebuild_index();
        Some(removed)
    }

    /// 按 tick 删除第一个匹配事件（O(log 块数 + 块内)）
    pub fn remove_by_tick(&mut self, tick: u32) -> Option<T>
    where
        T: Clone,
    {
        if self.chunks.is_empty() {
            return None;
        }
        let ci = self.locate_chunk(tick);
        let local = self.chunks[ci].partition_point(|e| e.tick() < tick);
        if self.chunks[ci].get(local).map(EventTick::tick) == Some(tick) {
            let removed = self.chunks[ci].remove(local);
            self.total_len -= 1;
            if self.chunks[ci].is_empty() && self.chunks.len() > 1 {
                self.chunks.remove(ci);
            }
            self.rebuild_index();
            Some(removed)
        } else {
            None
        }
    }

    /// 二分查找：第一个 tick >= 目标的事件全局索引（O(log 块数 + log 块内)）
    ///
    /// 与 `&[T].partition_point` 语义一致：返回满足 `e.tick() < tick` 的事件数。
    pub fn partition_point(&self, tick: u32) -> usize {
        if self.total_len == 0 {
            return 0;
        }
        let ci = self.locate_chunk(tick);
        let local = self.chunks[ci].partition_point(|e| e.tick() < tick);
        self.chunk_offsets[ci] + local
    }

    /// 返回 tick 在 [start_tick, end_tick) 范围内的事件迭代器
    pub fn range(&self, start_tick: u32, end_tick: u32) -> impl Iterator<Item = &T> {
        let start = self.partition_point(start_tick);
        let end = self.partition_point(end_tick);
        // 惰性跨块切片：start..end 可能跨块，用 flat_map + skip/take
        self.iter().skip(start).take(end.saturating_sub(start))
    }

    /// 小规模兼容：转换为 Vec（仅测试/低频路径使用）
    pub fn to_vec(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.iter().cloned().collect()
    }

    /// 区间访问（O(区间长度)），返回区间内事件的引用集合。
    ///
    /// 越界返回 None（与 `slice.get(range)` 语义一致）。
    /// 返回 `Vec<&T>` 而非切片：事件跨块存储，无法返回连续切片。
    pub fn get_range(&self, range: std::ops::RangeInclusive<usize>) -> Option<Vec<&T>> {
        let (start, end) = (*range.start(), *range.end());
        if start > end || end >= self.total_len {
            return None;
        }
        let count = end - start + 1;
        let mut result = Vec::with_capacity(count);
        for i in start..=end {
            result.push(self.get(i)?);
        }
        Some(result)
    }

    /// 定位 tick 所在块索引（O(log 块数)）
    ///
    /// 二分在 chunk_first_ticks 中找最后一个 first_tick <= tick 的块。
    /// tick 小于首块首事件时落到首块（saturating 防下溢）。
    fn locate_chunk(&self, tick: u32) -> usize {
        debug_assert!(!self.chunks.is_empty());
        self.chunk_first_ticks
            .partition_point(|&ft| ft <= tick)
            .saturating_sub(1)
    }

    /// 将 `ci` 块分裂为两个 25 万块（O(块内)）
    fn split_chunk(&mut self, ci: usize) {
        let chunk = &mut self.chunks[ci];
        let right: Vec<T> = chunk.split_off(EVENT_CHUNK_SPLIT);
        let right_first = right.first().map(EventTick::tick).unwrap_or(0);
        // 插入右块（ci+1 位置）
        self.chunks.insert(ci + 1, right);
        // 右块首 tick 索引：原 first_tick 不变（左块），右块首 tick 插入 ci+1
        self.chunk_first_ticks.insert(ci + 1, right_first);
        // chunk_offsets 在 rebuild_index 中重建
    }

    /// 重建双索引（O(块数)，插入/删除后调用；块数 ~120 时开销可忽略）
    fn rebuild_index(&mut self) {
        self.chunk_first_ticks = self
            .chunks
            .iter()
            .map(|c| c.first().map(EventTick::tick).unwrap_or(0))
            .collect();
        let mut offsets = Vec::with_capacity(self.chunks.len() + 1);
        offsets.push(0);
        let mut acc = 0usize;
        for c in &self.chunks {
            acc += c.len();
            offsets.push(acc);
        }
        self.chunk_offsets = offsets;
        debug_assert_eq!(acc, self.total_len);
    }
}

impl<T: EventTick> Default for ChunkedList<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// 支持 `for e in &list`（按 tick 升序遍历）
impl<'a, T: EventTick> IntoIterator for &'a ChunkedList<T> {
    type Item = &'a T;
    type IntoIter = std::iter::FlatMap<
        std::slice::Iter<'a, Vec<T>>,
        std::slice::Iter<'a, T>,
        fn(&'a Vec<T>) -> std::slice::Iter<'a, T>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.chunks.iter().flat_map(|c| c.iter())
    }
}

/// 与 `Vec<T>` 内容比较（便于测试与旧代码迁移：`assert_eq!(list, vec)`）
impl<T: EventTick + PartialEq> PartialEq<Vec<T>> for ChunkedList<T> {
    fn eq(&self, other: &Vec<T>) -> bool {
        self.total_len == other.len() && self.iter().eq(other.iter())
    }
}

/// 索引访问（全局索引，O(log 块数)）
impl<T: EventTick> std::ops::Index<usize> for ChunkedList<T> {
    type Output = T;

    fn index(&self, index: usize) -> &T {
        self.get(index).unwrap_or_else(|| {
            panic!(
                "ChunkedList index out of bounds: index={index}, len={}",
                self.total_len
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn ev(tick: u32, id: u32) -> TestEvent {
        TestEvent { tick, id }
    }

    fn sorted_events(count: usize) -> Vec<TestEvent> {
        (0..count as u32).map(|i| ev(i * 10, i)).collect()
    }

    /// 参照实现：普通 Vec + partition_point（验证 ChunkedList 行为等价）
    fn reference_insert(sorted: &mut Vec<TestEvent>, e: TestEvent) {
        let idx = sorted.partition_point(|x| x.tick <= e.tick);
        sorted.insert(idx, e);
    }

    #[test]
    fn test_from_sorted_basic() {
        let list = ChunkedList::from_sorted(sorted_events(10));
        assert_eq!(list.len(), 10);
        assert_eq!(list.first().unwrap().tick, 0);
        assert_eq!(list.last().unwrap().tick, 90);
        assert_eq!(list.get(5).unwrap().tick, 50);
        assert_eq!(list.get(9).unwrap().tick, 90);
        assert_eq!(list.get(10), None);
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
        list.insert(ev(50, 0));
        assert_eq!(list.len(), 1);
        assert_eq!(list.first().unwrap().tick, 50);
    }

    #[test]
    fn test_insert_middle_preserves_order() {
        let mut list = ChunkedList::from_sorted(sorted_events(10));
        // 插到 tick=50 之前（50 前插入 → 稳定插到同 tick 后）
        list.insert(ev(45, 99));
        let ticks: Vec<u32> = list.iter().map(EventTick::tick).collect();
        assert_eq!(ticks, vec![0, 10, 20, 30, 40, 45, 50, 60, 70, 80, 90]);
        // 同 tick 稳定插入
        list.insert(ev(45, 100));
        let ticks: Vec<u32> = list.iter().map(EventTick::tick).collect();
        assert_eq!(ticks, vec![0, 10, 20, 30, 40, 45, 45, 50, 60, 70, 80, 90]);
        assert_eq!(list.len(), 12);
    }

    #[test]
    fn test_insert_before_first_and_after_last() {
        let mut list = ChunkedList::from_sorted(sorted_events(10));
        list.insert(ev(5, 98)); // 首元素（tick=0）之后、第二元素之前
        list.insert(ev(95, 97)); // 末元素（tick=90）之后
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
            let e = ev(tick, i);
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
        list.insert(ev(EVENT_CHUNK_CAPACITY as u32 * 10 / 2, 999_999));
        assert_eq!(list.chunk_count(), 2, "满块插入应分裂为 2 块");
        assert_eq!(list.len(), EVENT_CHUNK_CAPACITY + 1);

        // 顺序保持
        let ticks: Vec<u32> = list.iter().map(EventTick::tick).collect();
        assert!(ticks.windows(2).all(|w| w[0] <= w[1]), "分裂后必须保持升序");

        // 每块事件数 ≤ 容量
        assert!(list.chunks.iter().all(|c| c.len() <= EVENT_CHUNK_CAPACITY));
        // get 跨块索引正确
        assert_eq!(
            list.get(EVENT_CHUNK_SPLIT).unwrap().tick,
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
            let e = ev(tick, i);
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
        let removed = list.remove(500).unwrap();
        assert_eq!(removed.tick, 5000);
        assert_eq!(list.len(), 999);
        assert_eq!(list.get(499).unwrap().tick, 4990);
        assert_eq!(list.get(500).unwrap().tick, 5010);

        // 按 tick 删除
        let removed = list.remove_by_tick(5010).unwrap();
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
        list.insert(ev(123, 1));
        list.insert(ev(456, 2));

        let range: Vec<u32> = list.range(100, 500).map(EventTick::tick).collect();
        assert_eq!(
            range,
            vec![
                100, 110, 120, 123, 130, 140, 150, 160, 170, 180, 190, 200, 210, 220, 230, 240,
                250, 260, 270, 280, 290, 300, 310, 320, 330, 340, 350, 360, 370, 380, 390, 400,
                410, 420, 430, 440, 450, 456, 460, 470, 480, 490
            ]
        );
    }

    #[test]
    fn test_large_insert_performance_shape() {
        // 验证 50 万规模插入仍是块内操作（不崩、顺序正确）
        let mut list = ChunkedList::from_sorted(sorted_events(EVENT_CHUNK_CAPACITY));
        list.insert(ev(3, 1));
        list.insert(ev(3, 2));
        assert_eq!(list.len(), EVENT_CHUNK_CAPACITY + 2);
        assert_eq!(list.first().unwrap().tick, 0);
        assert_eq!(list.get(2).unwrap().tick, 3);
    }

    #[test]
    fn test_replace_and_clear() {
        let mut list = ChunkedList::from_sorted(sorted_events(100));
        list.replace_sorted(sorted_events(200));
        assert_eq!(list.len(), 200);
        list.clear();
        assert!(list.is_empty());
        assert_eq!(list.chunk_count(), 0);
        list.insert(ev(1, 1));
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
}
