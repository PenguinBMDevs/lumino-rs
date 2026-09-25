//! `ChunkedList` 批量降序区间删除（自 `mutate.rs` 拆出，保持各文件 < 400 行）
//!
//! 2026-09 删除性能修复：逐音符 [`ChunkedList::remove`] 每次调用
//! `rebuild_index()`（O(块数) + 2 次分配），且非尾部区间的 `Vec::remove`
//! 搬移为 O(K×块长)。批量删除改为「每块一次 COW + 块内单遍压缩 + 单次索引重建」。

use std::sync::Arc;

use super::{ChunkedList, EventTick};

impl<T: EventTick> ChunkedList<T> {
    /// 批量删除降序区间（O(受影响块总长 + 块数)，单次索引重建）
    ///
    /// `ranges` 为 `(起始全局索引, 数量)`，需满足：按起始索引**降序**、
    /// 互不重叠（`merge_descending_ranges` 的输出格式）；尾部越界自动截断。
    ///
    /// 与逐音符 `remove` 的关键差异（2026-09 删除性能修复）：
    /// - 每块只经一次 `Arc::make_mut`（COW 快照共享时每块至多复制一次）；
    /// - 块内单遍原地压缩（保留段左移），无 K 次尾部搬移；
    /// - 全程只在末尾 `rebuild_index` 一次（旧路径每次 remove 重建 O(块数) + 2 次分配，
    ///   39 块工程逐音符删除约 250 ns/次，200W 次 ≈ 0.6s，且非尾部区间搬移为 O(K×块长)）。
    /// - 整块被删时直接丢弃块（零拷贝，全轨删除退化为 O(块数)）。
    ///
    /// 返回实际删除的事件数。
    pub fn remove_ranges(&mut self, ranges: &[(usize, usize)]) -> usize
    where
        T: Clone,
    {
        if ranges.is_empty() || self.total_len == 0 {
            return 0;
        }
        // 防御：非降序输入（调用方违约）就地规范化，避免块内压缩段下溢
        let normalized: Vec<(usize, usize)>;
        let ranges: &[(usize, usize)] = if ranges.windows(2).all(|w| w[0].0 > w[1].0) {
            ranges
        } else {
            let mut v = ranges.to_vec();
            v.sort_unstable_by_key(|r| std::cmp::Reverse(r.0));
            normalized = v;
            &normalized
        };
        let mut removed_total = 0usize;
        let mut empty_chunks: Vec<usize> = Vec::new();
        // 块内局部删除区间缓冲（升序、互不重叠；复用容量避免逐块分配）
        let mut local: Vec<(usize, usize)> = Vec::new();

        let mut ci = self.chunks.len();
        let mut ri = 0usize;
        while ci > 0 && ri < ranges.len() {
            ci -= 1;
            let off = self.chunk_offsets[ci];
            let clen = self.chunks[ci].len();
            let cend = off + clen;
            local.clear();
            // 收集与当前块相交的区间（ranges 降序 → 相交区间连续）
            let mut j = ri;
            while j < ranges.len() {
                let (s, count) = ranges[j];
                let e = s.saturating_add(count);
                if s >= cend {
                    // 区间起点在更高块：已处理完（含完全越界的尾部区间）→ 跳过
                    j += 1;
                    continue;
                }
                if e <= off {
                    // 完全位于更低块 → 后续区间更靠前，全部留待后续块
                    break;
                }
                // 与当前块相交
                let lo = s.max(off) - off;
                let hi = e.min(cend) - off;
                if hi > lo {
                    local.push((lo, hi));
                }
                if s < off {
                    // 区间起点在更低块 → 游标停在此区间，下一（更低）块继续处理其下半段
                    break;
                }
                j += 1;
            }
            ri = j;
            if local.is_empty() {
                continue;
            }
            local.reverse(); // ranges 降序 → 收集为降序，反转为升序
            let removed: usize = local.iter().map(|(l, h)| h - l).sum();

            // 整块删除：直接丢弃块（零拷贝，不复制整块）
            if local.len() == 1 && local[0] == (0, clen) {
                removed_total += removed;
                empty_chunks.push(ci);
                continue;
            }

            let chunk = Arc::make_mut(&mut self.chunks[ci]);
            if local.len() == 1 {
                // 单区间：一次 `drain`（块尾整体 memmove）替代逐元素压缩
                let (lo, hi) = local[0];
                chunk.drain(lo..hi);
            } else {
                // 单遍原地压缩：保留段左移（swap 免 Clone 约束；源位置随后不再读取）
                let mut read = 0usize;
                let mut write = 0usize;
                for &(lo, hi) in &local {
                    let keep = lo - read;
                    for k in 0..keep {
                        chunk.swap(read + k, write + k);
                    }
                    write += keep;
                    read = hi;
                }
                let tail = chunk.len() - read;
                for k in 0..tail {
                    chunk.swap(read + k, write + k);
                }
                write += tail;
                chunk.truncate(write);
            }
            removed_total += removed;
            if chunk.is_empty() {
                empty_chunks.push(ci);
            }
        }

        if removed_total == 0 {
            return 0;
        }
        self.total_len -= removed_total;
        // empty_chunks 按块索引降序收集：降序移除避免索引位移
        for &ci in &empty_chunks {
            self.chunks.remove(ci);
        }
        self.rebuild_index();
        removed_total
    }
}
