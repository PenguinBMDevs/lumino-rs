//! ChunkedList 查询操作：二分 / 范围 / 窗口 / 值定位
//!
//! 2026-08-06 阶段二拆分：原 `chunked_list.rs`（1192 行）按读写职责拆分，
//! 本模块承载全部只读查询路径。写路径见 `super::mutate`。

use super::{ChunkedList, EventTick};

impl<T: EventTick> ChunkedList<T> {
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
    pub fn iter_window<'a>(&'a self, lo: usize, hi: usize) -> super::iter::WindowIter<'a, T> {
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
        super::iter::WindowIter {
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
    ///
    /// `pub(crate)`：mutate 子模块的 insert / remove_by_tick 亦需定位。
    pub(crate) fn locate_chunk(&self, tick: u32) -> usize {
        debug_assert!(!self.chunks.is_empty());
        self.chunk_first_ticks
            .partition_point(|&ft| ft <= tick)
            .saturating_sub(1)
    }
}

impl ChunkedList<crate::note_event::NoteEvent> {
    /// 按值定位音符全局索引（O(log 块数 + 同 tick 事件数)，无全扫）。
    ///
    /// 与通用 [`Self::position_of`] 同语义，专供 NoteEvent 调用方语义明确化。
    pub fn position_of_value(&self, event: &crate::note_event::NoteEvent) -> Option<usize> {
        self.position_of(event)
    }
}

/// 按 tick 有序、可按全局索引访问的事件序列（[`Self::position_of_unused`] 的存储抽象）。
///
/// 存在的唯一理由：**让「同值多份按份数分配」这一原语只有一份实现**。
/// 该原语服务于两种截然不同的存储——[`ChunkedList<T>`]（跨轨分块，无法提供连续切片）
/// 与 `[T]`（异步提交线程中的整轨克隆副本）。若各自实现一份，同值多份场景下
/// 改一处漏一处即造成**静默丢份**（选中丢音符 / 撤销丢音符，无任何提示）。
///
/// 已实现：[`ChunkedList<T>`] 与 `[T]`。新增存储只需实现三个原语方法即可复用同一算法。
pub trait TickIndexedEvents {
    /// 事件类型（须可按 tick 排序且可全字段比较）
    type Event: EventTick + PartialEq;

    /// 事件总数
    fn event_count(&self) -> usize;

    /// 全局索引访问；越界返回 `None`
    fn event_at(&self, index: usize) -> Option<&Self::Event>;

    /// 首个 `tick() >= tick` 的全局索引（等价 `partition_point(|e| e.tick() < tick)`）
    fn first_index_at_tick(&self, tick: u32) -> usize;

    /// 同 tick 段内跳过已占用索引的按值定位（同值多份按份数分配）。
    ///
    /// 先定位到 `target` 所在 tick 段首，再在该段内线性扫描**全字段匹配**
    /// （`PartialEq`）且未被 `used` 占用的首个索引；段内未命中即返回 `None`，
    /// **无全扫兜底**（跨段扫描会命中错误音符，宁可取消选中）。
    ///
    /// 调用方负责把返回索引插入 `used`，以保证同值多份各分配到不同索引。
    fn position_of_unused(
        &self,
        target: &Self::Event,
        used: &std::collections::HashSet<usize>,
    ) -> Option<usize> {
        let mut i = self.first_index_at_tick(target.tick());
        let len = self.event_count();
        while i < len {
            let Some(n) = self.event_at(i) else {
                break;
            };
            // 段内才做全字段比较：越过本段即终止（无跨段兜底）
            if n.tick() != target.tick() {
                break;
            }
            if n == target && !used.contains(&i) {
                return Some(i);
            }
            i += 1;
        }
        None
    }
}

impl<T: EventTick + PartialEq> TickIndexedEvents for ChunkedList<T> {
    type Event = T;

    #[inline]
    fn event_count(&self) -> usize {
        self.len()
    }

    #[inline]
    fn event_at(&self, index: usize) -> Option<&T> {
        self.get(index)
    }

    #[inline]
    fn first_index_at_tick(&self, tick: u32) -> usize {
        // 分块容器的真二分（块级 + 块内），首个 tick >= 目标
        self.partition_point(tick)
    }
}

impl<T: EventTick + PartialEq> TickIndexedEvents for [T] {
    type Event = T;

    #[inline]
    fn event_count(&self) -> usize {
        self.len()
    }

    #[inline]
    fn event_at(&self, index: usize) -> Option<&T> {
        self.get(index)
    }

    #[inline]
    fn first_index_at_tick(&self, tick: u32) -> usize {
        // 与 ChunkedList::partition_point 同语义：首个 tick >= 目标
        self.partition_point(|e| e.tick() < tick)
    }
}
