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
//! 2026-08-06 阶段二拆分（原文件 1192 行 → 按职责分模块）：
//! - `mutate`：写路径（insert/remove/push_back/分裂/索引重建）
//! - `query`：只读路径（二分/范围/窗口/值定位）
//! - `iter`：跨块窗口迭代器与遍历支持
//! - `tests`：单元测试
//!
//! 设计要点：
//! - 块间与块内均按 tick 升序，块级二分（`chunk_first_ticks`）+ 块内二分（`partition_point`）
//! - `insert` 只移动目标块内元素（≤ 25 万），跨块 O(log 块数) 定位
//! - `partition_point` 为真二分（块级 + 块内），播放引擎 seek 热路径依赖
//! - 泛型 T 只需实现 `EventTick`（提供 `tick()`）

mod iter;
mod mutate;
mod query;
#[cfg(test)]
mod tests;

use std::sync::Arc;

pub use iter::WindowIter;

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
        let mut chunk_offsets = Vec::with_capacity(chunks.capacity());
        let mut acc = 0usize;

        let mut total_len = 0usize;
        let mut iter = events.into_iter();
        loop {
            let chunk: Vec<T> = iter.by_ref().take(EVENT_CHUNK_CAPACITY).collect();
            if chunk.is_empty() {
                break;
            }
            let first_tick = chunk.first().map(EventTick::tick).unwrap_or(0);
            let chunk_len = chunk.len();
            chunk_offsets.push(acc); // 第 i 块起始全局索引
            acc += chunk_len;
            chunks.push(Arc::new(chunk));
            chunk_first_ticks.push(first_tick);
            total_len += chunk_len;
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
    ///
    /// 不变式：`chunk_offsets.len() == chunks.len()`，
    /// `chunk_offsets[i]` 为第 `i` 块起始全局索引（第 0 块恒为 0）。
    /// 因此 `EMPTY`（双空）天然满足不变式，无需哨兵 `vec![0]`，
    /// 杜绝了旧不变式下 `EMPTY` 越界减法（release 下 `usize::MAX`）的隐患。
    pub fn new() -> Self {
        Self {
            chunks: Vec::new(),
            chunk_first_ticks: Vec::new(),
            chunk_offsets: Vec::new(),
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
            chunk_offsets: Vec::new(),
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
