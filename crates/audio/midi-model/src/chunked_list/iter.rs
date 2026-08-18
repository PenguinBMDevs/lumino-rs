//! ChunkedList 跨块窗口迭代器与遍历支持
//!
//! 2026-08-06 阶段二拆分：原 `chunked_list.rs`（1192 行）按职责拆分，
//! 本模块承载 `WindowIter` 类型与 `&ChunkedList` 的 `IntoIterator` 实现。

use std::sync::Arc;

use super::{ChunkedList, EventTick};

/// 跨块惰性窗口迭代器：产出含全局索引的 `(index, &T)` 对，仅访问 `[lo, hi)` 窗口
///
/// 由 [`ChunkedList::iter_window`] 创建。经 `chunk_offsets` 块级跳变直接
/// 定位起始块，规避 `iter().skip(lo)` 在窗口前的 O(lo) 平铺扫描。
pub struct WindowIter<'a, T> {
    /// 分块引用（借自容器）
    pub(super) chunks: &'a [Arc<Vec<T>>],
    /// 当前块索引
    pub(super) cur_ci: usize,
    /// 当前块内偏移
    pub(super) cur_local: usize,
    /// 当前全局索引
    pub(super) cur_global: usize,
    /// 窗口上界（全局索引，含）
    pub(super) end: usize,
    /// 迭代终止标记（防止 end == total_len 时继续越过）
    pub(super) done: bool,
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
