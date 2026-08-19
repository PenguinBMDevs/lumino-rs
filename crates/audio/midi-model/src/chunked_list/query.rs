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
