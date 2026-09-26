//! 冻结音符集：精确 `(视觉音轨, start_tick, key)` 成员判定。
//!
//! 走带选区在几何变更（拖动平移 / 批量变速）后，矩形会顺带覆盖落点区域内**本来不在
//! 框选内**的既有音符。本模块把「框选时内部包含的音符」固定为精确集合，使 `contains`
//! 走成员判定而非矩形判定，从根上消除误伤。
//!
//! 紧凑表示：每个视觉音轨一条升序 `u64` 序列，元素为
//! `((start_tick as u64) << 8) | key`（8 字节/音符，二分查找 O(log K)）。
//!
//! 2026-09 自 `arrange_selection.rs` 拆出：该类型自洽，且与 `ArrangeSelection` 的
//! 矩形几何逻辑正交，拆分以满足 ≤400 行约束。

use std::collections::HashMap;

/// 冻结音符集：精确 `(视觉音轨, start_tick, key)` 成员判定。
#[derive(Clone, Default, Debug)]
pub struct FrozenNotes {
    /// 视觉音轨 → 升序打包键
    by_track: HashMap<u16, Vec<u64>>,
    /// 冻结音符总数（按 (start_tick, key) 去重后）
    len: usize,
}

/// `(start_tick, key)` 打包为可排序的 `u64`
#[inline]
fn pack(start_tick: u32, key: u8) -> u64 {
    ((start_tick as u64) << 8) | key as u64
}

impl FrozenNotes {
    /// 是否为空
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// 冻结音符数（去重后）
    pub fn len(&self) -> usize {
        self.len
    }

    /// 清空
    pub fn clear(&mut self) {
        self.by_track.clear();
        self.len = 0;
    }

    /// 由 `(视觉音轨, start_tick, key)` 迭代器构建（去重 + 按音轨升序）
    pub fn from_entries<I: IntoIterator<Item = (u16, u32, u8)>>(entries: I) -> Self {
        let mut by_track: HashMap<u16, Vec<u64>> = HashMap::new();
        for (track, start_tick, key) in entries {
            by_track
                .entry(track)
                .or_default()
                .push(pack(start_tick, key));
        }
        let mut len = 0usize;
        for keys in by_track.values_mut() {
            keys.sort_unstable();
            keys.dedup();
            len += keys.len();
        }
        Self { by_track, len }
    }

    /// 精确成员判定（按打包键二分，O(log K)）
    pub fn contains(&self, track: u16, start_tick: u32, key: u8) -> bool {
        let Some(keys) = self.by_track.get(&track) else {
            return false;
        };
        keys.binary_search(&pack(start_tick, key)).is_ok()
    }

    /// 按变换函数重建（整体偏移等几何变更用，闭包返回新 `(音轨, start_tick, key)`）
    pub fn transform<F>(&mut self, mut f: F)
    where
        F: FnMut(u16, u32, u8) -> (u16, u32, u8),
    {
        let mut by_track: HashMap<u16, Vec<u64>> = HashMap::with_capacity(self.by_track.len());
        for (&track, keys) in &self.by_track {
            for &packed in keys {
                let (nt, ns, nk) = f(track, (packed >> 8) as u32, packed as u8);
                by_track.entry(nt).or_default().push(pack(ns, nk));
            }
        }
        let mut len = 0usize;
        for keys in by_track.values_mut() {
            keys.sort_unstable();
            keys.dedup();
            len += keys.len();
        }
        self.by_track = by_track;
        self.len = len;
    }
}
