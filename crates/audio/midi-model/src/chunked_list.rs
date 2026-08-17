//! ChunkedList — 泛型分块有序事件容器
//!
//! 2026-08-06 阶段二：解决超大单一 Vec 插入阻塞（O(N) memmove）。
//! 按 tick 有序的事件序列（音符 / CC/PC/PB 控制事件）统一使用本容器：
//! 每块最多 50 万事件，满块插入时动态分裂为两个 25 万块。
//!
//! 2026-08-06 内存修复：块级 `Arc<Vec<T>>` 写时复制（COW）。
//! - `clone()` 退化为 O(块数) 指针拷贝，块数据物理共享（未修改的块在所有快照间共址）
//! - 修改路径（insert/remove/get_mut/push_back/split）经 `Arc::make_mut` 只复制目标块
//! - 撤销/重做快照从此不拷贝整轨数据，1600W 音符工程快照内存 O(块数) 而非 O(N)
//!
//! 设计要点：
//! - 块间与块内均按 tick 升序，块级二分（`chunk_first_ticks`）+ 块内二分（`partition_point`）
//! - `insert` 只移动目标块内元素（≤ 25 万），跨块 O(log 块数) 定位
//! - `partition_point` 为真二分（块级 + 块内），播放引擎 seek 热路径依赖
//! - 泛型 T 只需实现 `EventTick`（提供 `tick()`）

use std::sync::Arc;

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

/// 泛型分块有序事件容器（块级 Arc 写时复制）
pub struct ChunkedList<T> {
    /// 分块，块间按首事件 tick 升序，块内按 tick 升序。
    /// `Arc` 共享：clone 为 O(块数) 指针拷贝，未修改块在快照间物理共址，
    /// 修改路径经 `Arc::make_mut` 写时复制（只复制目标块）。
    chunks: Vec<Arc<Vec<T>>>,
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
        let mut chunks: Vec<Arc<Vec<T>>> =
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
            chunks.push(Arc::new(chunk));
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

    /// 从已按 tick 升序排列的迭代器构建分块容器（O(N)）。
    ///
    /// 与 [`Self::from_sorted`] 语义一致，但直接消费迭代器分块收集，
    /// **不产生中间 `Vec<T>`** —— 用于 MIDI 加载路径中
    /// `PackedNote → NoteEvent` 的转换，避免 `Vec<PackedNote>` 与
    /// `Vec<NoteEvent>` 同时驻留（2.9 亿音符场景可省 ~4.6GB 峰值）。
    ///
    /// 注意：调用方需保证迭代器按 tick 升序（同 [`Self::from_sorted`]）。
    pub fn from_sorted_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut iter = iter.into_iter();
        let mut chunks: Vec<Arc<Vec<T>>> = Vec::new();
        let mut chunk_first_ticks = Vec::new();
        let mut chunk_offsets = vec![0];
        let mut total_len = 0usize;

        loop {
            let chunk: Vec<T> = iter.by_ref().take(EVENT_CHUNK_CAPACITY).collect();
            if chunk.is_empty() {
                break;
            }
            debug_assert!(
                chunk.windows(2).all(|w| w[0].tick() <= w[1].tick()),
                "from_sorted_iter 要求迭代器按 tick 升序"
            );
            let first_tick = chunk.first().map(EventTick::tick).unwrap_or(0);
            let chunk_len = chunk.len();
            chunks.push(Arc::new(chunk));
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
        self.chunks.iter().map(|c| c.capacity()).sum()
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

    /// 全局索引可变访问（O(log 块数) + 目标块 COW 拷贝）
    ///
    /// 返回目标块内的可变引用，必要时经 `Arc::make_mut` 复制目标块
    /// （快照共享时只复制该块）。越界返回 None。
    /// 注意：修改事件 tick 会破坏排序不变式，调用方需自行保证（与旧 `&mut Vec` 语义一致）。
    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut T>
    where
        T: Clone,
    {
        if index >= self.total_len {
            return None;
        }
        let ci = self.chunk_offsets.partition_point(|&o| o <= index) - 1;
        let local = index - self.chunk_offsets[ci];
        let chunk = Arc::make_mut(&mut self.chunks[ci]);
        chunk.get_mut(local)
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
    pub fn push_back(&mut self, event: T)
    where
        T: Clone,
    {
        // 快速路径：末尾块存在、非空、未满、且事件 tick 不早于末事件
        let fast_path = self.chunks.last().is_some_and(|last| {
            last.last().is_some_and(|e| e.tick() <= event.tick())
                && last.len() < EVENT_CHUNK_CAPACITY
        });
        if fast_path {
            // fast_path 已保证末尾块非空（is_some_and 前置检查）；
            // if-let 防御性兜底，避免 expect panic 路径。
            if let Some(last) = self.chunks.last_mut() {
                let last = Arc::make_mut(last);
                last.push(event);
                self.total_len += 1;
                if let Some(last_offset) = self.chunk_offsets.last_mut() {
                    *last_offset = self.total_len;
                }
            }
            return;
        }
        // 兜底：空容器 / 空块 / 末尾块满 / 乱序 → 走有序插入（内部处理分裂与索引）
        self.insert(event);
    }

    /// 按 tick 升序插入事件（O(块内) + O(log 块数)）
    ///
    /// 定位目标块后块内二分插入；若目标块已满（50 万），先分裂为两个
    /// 25 万块，再插入目标半块。必要时经 `Arc::make_mut` 只复制目标块。
    pub fn insert(&mut self, event: T)
    where
        T: Clone,
    {
        let tick = event.tick();
        if self.chunks.is_empty() {
            let chunk = Arc::new(vec![event]);
            self.chunks.push(chunk);
            self.chunk_first_ticks.push(tick);
            self.chunk_offsets = vec![0, 1];
            self.total_len = 1;
            return;
        }

        let ci = self.locate_chunk(tick);

        if self.chunks[ci].len() >= EVENT_CHUNK_CAPACITY {
            // 满块分裂：切成两个 25 万块，插入目标半块
            self.split_chunk(ci);
            let ci = self.locate_chunk(tick);
            let chunk = Arc::make_mut(&mut self.chunks[ci]);
            let local = chunk.partition_point(|e| e.tick() <= tick);
            chunk.insert(local, event);
        } else {
            let chunk = Arc::make_mut(&mut self.chunks[ci]);
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
        let chunk = Arc::make_mut(&mut self.chunks[ci]);
        let removed = chunk.remove(local);
        self.total_len -= 1;
        // 空块清理（保留至少一个块用于索引一致性）
        if chunk.is_empty() && self.chunks.len() > 1 {
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
        let target = {
            let chunk = &self.chunks[ci];
            let local = chunk.partition_point(|e| e.tick() < tick);
            if chunk.get(local).map(EventTick::tick) == Some(tick) {
                Some(local)
            } else {
                None
            }
        };
        if let Some(local) = target {
            let chunk = Arc::make_mut(&mut self.chunks[ci]);
            let removed = chunk.remove(local);
            self.total_len -= 1;
            if chunk.is_empty() && self.chunks.len() > 1 {
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

    /// 视窗定位：返回 tick 在 `[start_tick - lookback_ticks, end_tick)` 的全局索引区间 `(lo, hi)`
    ///
    /// 与 [`Self::range`]（严格 `start_tick` 窗口）不同，本方法额外向左扩展
    /// `lookback_ticks`，用于「跨入查询」——钢琴卷帘可见性/点选需要
    /// `start_tick <= 查询位置 <= start_tick + length` 的音符，而容器按
    /// `start_tick` 排序，命中一个长音符可能从其起点向左横跨很远。向前看
    /// 一个安全上界（lookback）即可在 O(log 块数) 内框出含跨入音符的考察区间。
    ///
    /// 注意：lookback 为近似。极端超长音符（长度超过 lookback 的跨度）仍会
    /// 落在区间之外——调用方应结合业务约束选择足够大的 lookback。
    ///
    /// 复杂度 O(log 块数)（两个 `partition_point`）。
    #[inline]
    pub fn window_range(
        &self,
        start_tick: u32,
        end_tick: u32,
        lookback_ticks: u32,
    ) -> (usize, usize) {
        let lo_tick = start_tick.saturating_sub(lookback_ticks);
        (
            self.partition_point(lo_tick),
            self.partition_point(end_tick),
        )
    }

    /// 在含全局索引的跨块窗口 `[lo, hi)` 上惰性迭代（替代 `iter().skip(lo)`，后者 O(N) 扫描）
    ///
    /// `skip(lo)` 在 1600W 前的块上跳过 O(lo) 平铺搜集，代价 O(N)。本迭代器经
    /// `chunk_offsets` 块级跳变直接定位 lo 所在块，只访问窗口内的元素，
    /// 总计 O(log 块数 + 窗口长度)。供钢琴卷帘视口/命中查询使用。
    pub fn iter_window<'a>(&'a self, lo: usize, hi: usize) -> WindowIter<'a, T> {
        let hi = hi.min(self.total_len);
        let lo = lo.min(hi);
        let (cur_ci, cur_local) = if self.chunks.is_empty() {
            // 空容器：迭代器立即终止（next 检查 cur_global >= end）
            (0, 0)
        } else {
            let ci = self
                .chunk_offsets
                .partition_point(|&o| o <= lo)
                .saturating_sub(1);
            (ci, lo.saturating_sub(self.chunk_offsets[ci]))
        };
        WindowIter {
            chunks: &self.chunks,
            cur_ci,
            cur_local,
            cur_global: lo,
            end: hi,
            done: false,
        }
    }

    /// 按值查找事件全局索引（O(log 块数 + 同 tick 事件数)）
    ///
    /// 定位目标 tick 所在块后，从块内首段顺序扫描精确匹配（`PartialEq` 全字段）。
    /// 用于 NoteCreate 增量日志 undo 时按值精确删除（无需记录插入索引，
    /// 不受后续同轨操作导致的索引漂移影响）。未找到返回 None。
    pub fn position_of(&self, event: &T) -> Option<usize>
    where
        T: PartialEq,
    {
        if self.total_len == 0 {
            return None;
        }
        let ci = self.locate_chunk(event.tick());
        let mut local_begin = self.chunks[ci].partition_point(|e| e.tick() < event.tick());
        let mut global = self.chunk_offsets[ci] + local_begin;
        // 从起始块扫到末尾（同 tick 事件通常极少，跨块续扫极少发生）
        for chunk in &self.chunks[ci..] {
            for e in &chunk[local_begin..] {
                if e.tick() > event.tick() {
                    return None;
                }
                if e == event {
                    return Some(global);
                }
                global += 1;
            }
            local_begin = 0;
        }
        None
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
    fn split_chunk(&mut self, ci: usize)
    where
        T: Clone,
    {
        let left = Arc::make_mut(&mut self.chunks[ci]);
        let right: Vec<T> = left.split_off(EVENT_CHUNK_SPLIT);
        let right_first = right.first().map(EventTick::tick).unwrap_or(0);
        // 插入右块（ci+1 位置）
        self.chunks.insert(ci + 1, Arc::new(right));
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

/// 跨块惰性窗口迭代器：产出含全局索引的 `(index, &T)` 对，仅访问 `[lo, hi)` 窗口
///
/// 由 [`ChunkedList::iter_window`] 创建。经 `chunk_offsets` 块级跳变直接
/// 定位起始块，规避 `iter().skip(lo)` 在窗口前的 O(lo) 平铺扫描。
pub struct WindowIter<'a, T> {
    chunks: &'a [Arc<Vec<T>>],
    /// 当前块索引
    cur_ci: usize,
    /// 当前块内偏移
    cur_local: usize,
    /// 当前全局索引
    cur_global: usize,
    /// 窗口上界（全局索引，含）
    end: usize,
    /// 迭代终止标记（防止 end == total_len 时继续越过）
    done: bool,
}

impl<'a, T: EventTick> Iterator for WindowIter<'a, T> {
    type Item = (usize, &'a T);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        // 前跳空块（lo 边界可能落在某块被清空后的缺口）
        while self.cur_ci < self.chunks.len() && self.cur_local >= self.chunks[self.cur_ci].len() {
            self.cur_ci += 1;
            self.cur_local = 0;
        }
        if self.cur_ci >= self.chunks.len() || self.cur_global >= self.end {
            self.done = true;
            return None;
        }
        let chunk = &self.chunks[self.cur_ci];
        let item = (self.cur_global, &chunk[self.cur_local]);
        self.cur_global += 1;
        self.cur_local += 1;
        if self.cur_global >= self.end {
            self.done = true;
        }
        Some(item)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        if self.done {
            return (0, Some(0));
        }
        let remain = self.end.saturating_sub(self.cur_global);
        (remain, Some(remain))
    }
}

/// 浅拷贝 O(块数)：块 Arc 共享，未修改块物理共址（undo 快照依赖此特性）
impl<T: EventTick> Clone for ChunkedList<T> {
    fn clone(&self) -> Self {
        Self {
            chunks: self.chunks.clone(),
            chunk_first_ticks: self.chunk_first_ticks.clone(),
            chunk_offsets: self.chunk_offsets.clone(),
            total_len: self.total_len,
        }
    }
}

impl<T: EventTick> std::fmt::Debug for ChunkedList<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChunkedList")
            .field("len", &self.total_len)
            .field("chunks", &self.chunks.len())
            .field("first", &self.first().map(EventTick::tick))
            .field("last", &self.last().map(EventTick::tick))
            .finish()
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
        std::slice::Iter<'a, Arc<Vec<T>>>,
        std::slice::Iter<'a, T>,
        fn(&'a Arc<Vec<T>>) -> std::slice::Iter<'a, T>,
    >;

    fn into_iter(self) -> Self::IntoIter {
        fn flat_map<'b, U: EventTick>(c: &'b Arc<Vec<U>>) -> std::slice::Iter<'b, U> {
            c.iter()
        }
        self.chunks.iter().flat_map(flat_map::<T>)
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

    fn make_test_event(tick: u32, id: u32) -> TestEvent {
        TestEvent { tick, id }
    }

    fn sorted_events(count: usize) -> Vec<TestEvent> {
        (0..count as u32).map(|i| make_test_event(i * 10, i)).collect()
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
        assert_eq!(list.get(499_999).expect("块 0 末元素应存在").tick, 4_999_990);
        assert_eq!(list.get(500_000).expect("块 1 首元素应存在").tick, 5_000_000);
        assert_eq!(
            list.get(699_999).expect("末元素应存在").tick,
            6_999_990
        );
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
        assert_eq!(list.position_of(&make_test_event(50, 999)), None, "同 tick 但 id 不匹配");
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
        let idx = list.position_of(&make_test_event(20, 2)).expect("应定位到目标事件");
        let removed = list.remove(idx).expect("目标事件应存在");
        assert_eq!(removed, make_test_event(20, 2));
        assert_eq!(list.position_of(&make_test_event(20, 2)), None);
        assert_eq!(list.position_of(&make_test_event(40, 4)), Some(1));
    }

    #[test]
    fn test_position_of_across_chunk_boundary() {
        // 用小块容量强制跨块：VENT_CHUNK_CAPACITY 是 const，这里用手工多事件验证
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
        list.insert(make_test_event(EVENT_CHUNK_CAPACITY as u32 * 10 / 2, 999_999));
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
            snapshot.iter().map(EventTick::tick).last().expect("快照末元素应存在"),
            6_999_990
        );
    }
}
