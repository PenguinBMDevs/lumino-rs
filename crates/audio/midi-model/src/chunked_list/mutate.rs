//! ChunkedList 修改操作：插入 / 删除 / 替换 / 块分裂与索引维护
//!
//! 2026-08-06 阶段二拆分：原 `chunked_list.rs`（1192 行）按读写职责拆分，
//! 本模块承载全部写路径。读路径见 `super::query`，迭代器见 `super::iter`。

use std::sync::Arc;

use super::{ChunkedList, EVENT_CHUNK_CAPACITY, EVENT_CHUNK_SPLIT, EventTick};

impl<T: EventTick> ChunkedList<T> {
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
            }
            return;
        }
        // 兜底：空容器 / 空块 / 末尾块满 / 乱序 → 走有序插入（内部处理分裂与索引）
        self.insert(event);
    }

    /// 按 tick 升序插入事件。
    ///
    /// 定位目标块后块内二分插入；目标块未满时**增量维护**索引
    /// （仅 `chunk_offsets[ci+1..]` 各 +1，O(1) 摊还），不再每次全量
    /// `rebuild_index`。目标块已满（50 万）时先分裂再插入，块数变化故仍全量重建。
    /// 必要时经 `Arc::make_mut` 只复制目标块。
    pub fn insert(&mut self, event: T)
    where
        T: Clone,
    {
        let tick = event.tick();
        if self.chunks.is_empty() {
            let chunk = Arc::new(vec![event]);
            self.chunks.push(chunk);
            self.chunk_first_ticks.push(tick);
            self.chunk_offsets = vec![0];
            self.total_len = 1;
            return;
        }

        let ci = self.locate_chunk(tick);

        if self.chunks[ci].len() >= EVENT_CHUNK_CAPACITY {
            // 满块分裂：切成两个 25 万块，插入目标半块（块数变化，全量重建索引）
            self.split_chunk(ci);
            let ci = self.locate_chunk(tick);
            let chunk = Arc::make_mut(&mut self.chunks[ci]);
            let local = chunk.partition_point(|e| e.tick() <= tick);
            chunk.insert(local, event);
            self.total_len += 1;
            self.rebuild_index();
            return;
        }

        let chunk = Arc::make_mut(&mut self.chunks[ci]);
        let local = chunk.partition_point(|e| e.tick() <= tick);
        chunk.insert(local, event);
        self.total_len += 1;
        // 增量维护：插入点之后的块起始索引整体 +1；第 0 块隐含 0，无需调整
        for off in &mut self.chunk_offsets[ci + 1..] {
            *off += 1;
        }
        if local == 0 {
            self.chunk_first_ticks[ci] = tick;
        }
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
        self.chunk_offsets = Vec::new();
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

    /// 批量插入已排序事件（O(N+M) 分块局部归并，单次重建，内存可控）
    ///
    /// `events` 需按 `tick` 升序（调用方保证，稳定插入语义：同 tick 排在已有事件之后）。
    /// 策略：仅归并受影响块区间（`min_tick..max_tick` 覆盖的块），
    /// 前缀/后缀块 `Arc` 浅拷零拷贝，避免全轨扫描 15M。
    /// 追加/前插（`min>old_last` / `max<old_first`）→ 零扫描拼接，O(块数)。
    pub fn extend_sorted(&mut self, events: Vec<T>)
    where
        T: Clone,
    {
        if events.is_empty() {
            return;
        }
        if self.is_empty() {
            *self = Self::from_sorted(events);
            return;
        }
        let min_tick = events.first().map(|e| e.tick()).unwrap_or(0);
        let max_tick = events.last().map(|e| e.tick()).unwrap_or(0);
        let old_first = self.first().map(|e| e.tick()).unwrap_or(0);
        let old_last = self.last().map(|e| e.tick()).unwrap_or(0);

        // 追加：新事件全部在尾部之后 → 旧块浅拷 + 新块拼接，零扫描
        if min_tick > old_last {
            let old = std::mem::take(self);
            let old_len = old.total_len;
            let new_list = Self::from_sorted(events);
            let new_len = new_list.total_len;
            let mut new_chunks = Vec::with_capacity(old.chunks.len() + new_list.chunks.len());
            new_chunks.extend(old.chunks);
            new_chunks.extend(new_list.chunks);
            *self = Self {
                chunks: new_chunks,
                chunk_first_ticks: Vec::new(),
                chunk_offsets: Vec::new(),
                total_len: old_len + new_len,
            };
            self.rebuild_index();
            return;
        }
        // 前插：新事件全部在首部之前 → 新块 + 旧块拼接，零扫描
        if max_tick < old_first {
            let old = std::mem::take(self);
            let old_len = old.total_len;
            let new_list = Self::from_sorted(events);
            let new_len = new_list.total_len;
            let mut new_chunks = Vec::with_capacity(new_list.chunks.len() + old.chunks.len());
            new_chunks.extend(new_list.chunks);
            new_chunks.extend(old.chunks);
            *self = Self {
                chunks: new_chunks,
                chunk_first_ticks: Vec::new(),
                chunk_offsets: Vec::new(),
                total_len: old_len + new_len,
            };
            self.rebuild_index();
            return;
        }

        // 通用局部归并：仅扫描受影响块区间
        let old = std::mem::take(self);
        let start_ci = old.locate_chunk(min_tick);
        let end_ci = old.locate_chunk(max_tick);

        // 前缀/后缀浅拷（Arc clone，零拷贝）
        let prefix_len = start_ci;
        let suffix_start = end_ci + 1;

        // 收集受影响区间旧事件（仅该区间，非全轨）— 用 extend_from_slice 做批量 memcpy
        let middle_old_len: usize = old.chunks[start_ci..=end_ci].iter().map(|c| c.len()).sum();
        let mut middle_old: Vec<T> = Vec::with_capacity(middle_old_len);
        for chunk in &old.chunks[start_ci..=end_ci] {
            middle_old.extend_from_slice(chunk);
        }
        // middle_old 已有序（块间/块内有序），events 已有序 → 线性归并
        let mut merged: Vec<T> = Vec::with_capacity(middle_old.len() + events.len());
        let mut i = 0usize;
        let mut j = 0usize;
        while i < middle_old.len() && j < events.len() {
            if middle_old[i].tick() <= events[j].tick() {
                merged.push(middle_old[i].clone());
                i += 1;
            } else {
                merged.push(events[j].clone());
                j += 1;
            }
        }
        if i < middle_old.len() {
            merged.extend_from_slice(&middle_old[i..]);
        }
        if j < events.len() {
            merged.extend_from_slice(&events[j..]);
        }

        // 中间区间重建分块（按 500k 切块）
        let middle_new = Self::from_sorted(merged);
        let mut new_chunks: Vec<Arc<Vec<T>>> = Vec::with_capacity(
            prefix_len + middle_new.chunks.len() + old.chunks.len() - suffix_start,
        );
        // 前缀浅拷
        new_chunks.extend(old.chunks[0..prefix_len].iter().cloned());
        // 中间新块
        new_chunks.extend(middle_new.chunks);
        // 后缀浅拷
        if suffix_start < old.chunks.len() {
            new_chunks.extend(old.chunks[suffix_start..].iter().cloned());
        }

        let total_len = old.total_len + events.len();
        *self = Self {
            chunks: new_chunks,
            chunk_first_ticks: Vec::new(),
            chunk_offsets: Vec::new(),
            total_len,
        };
        self.rebuild_index();
    }

    /// 批量插入（自动排序 + 归并，O((N+M)logM + N+M)）
    ///
    /// `events` 无需有序，内部按 `tick` 排序后走 `extend_sorted` 单次归并。
    /// 适合粘贴/放置等无序批量场景；已排序场景请直接用 `extend_sorted` 避免重复排序。
    pub fn batch_insert(&mut self, mut events: Vec<T>)
    where
        T: Clone,
    {
        if events.is_empty() {
            return;
        }
        events.sort_by_key(|a| a.tick());
        self.extend_sorted(events);
    }

    /// 重建双索引（O(块数)，块数变化后调用；块数 ~120 时开销可忽略）
    ///
    /// 不变式：`chunk_offsets.len() == chunks.len()`，且
    /// `chunk_offsets[i]` 为第 `i` 块起始全局索引（第 0 块恒为 0）。
    ///
    /// `pub(crate)`：tests 子模块的 `multi_chunk` 辅助构建多块容器后重建索引。
    pub(crate) fn rebuild_index(&mut self) {
        self.chunk_first_ticks = self
            .chunks
            .iter()
            .map(|c| c.first().map(EventTick::tick).unwrap_or(0))
            .collect();
        let mut offsets = Vec::with_capacity(self.chunks.len());
        let mut acc = 0usize;
        for c in &self.chunks {
            offsets.push(acc);
            acc += c.len();
        }
        self.chunk_offsets = offsets;
        debug_assert_eq!(self.chunk_offsets.len(), self.chunks.len());
        debug_assert_eq!(acc, self.total_len);
    }
}
