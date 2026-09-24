//! ChunkedList 有序不变式恢复：就地修改 tick 后重排
//!
//! 拖动流式应用 / undo-redo MoveOp 回放 / 量化等路径为性能**就地**修改
//! `start_tick`（`get_mut`），这会破坏「按 tick 升序」不变式——而
//! `partition_point` / `window_range` / `position_of_id` 快路径均依赖二分，
//! 破坏后渲染可见性、命中检测、框选会**漏检音符**（非仅性能退化）。
//! 本模块提供恢复原语，供各就地修改路径在修改后调用。
//!
//! 快路径依据：未移动元素之间相对顺序未变且原本有序，任何逆序对必涉及
//! 至少一个被移动元素——因此「每个被移动元素与当前左右邻居有序」⇔ 全表有序。

use super::{ChunkedList, EventTick};

/// 小集合阈值：不超过此值走 `remove` + `insert`（O(k·块内)），
/// 超过则走稳定归并重建（O(N + k log k)）。
const SMALL_MOVED_THRESHOLD: usize = 16;

/// 归一化被移动索引：升序、去重、越界剔除
fn normalize_moved(moved: &[usize], len: usize) -> Vec<usize> {
    let mut v: Vec<usize> = moved.to_vec();
    v.sort_unstable();
    v.dedup();
    v.retain(|&i| i < len);
    v
}

/// O(k) 局部有序检查（切片版）：`moved` 中每个元素与当前左右邻居有序 ⇒ 全表有序。
///
/// 契约：`moved` 必须包含**全部**被就地修改的元素（漏传可能漏检逆序对）。
pub fn sorted_locally_in_slice<T: EventTick>(all: &[T], moved: &[usize]) -> bool {
    moved.iter().all(|&i| {
        let cur = all.get(i).map(EventTick::tick);
        let prev = i
            .checked_sub(1)
            .and_then(|p| all.get(p))
            .map(EventTick::tick);
        let next = all.get(i + 1).map(EventTick::tick);
        match (prev, cur, next) {
            (Some(p), Some(c), Some(n)) => p <= c && c <= n,
            (None, Some(c), Some(n)) => c <= n,
            (Some(p), Some(c), None) => p <= c,
            _ => true,
        }
    })
}

/// 稳定归并恢复切片有序：`moved`（修改前索引，乱序可重复）处元素已就地改 tick。
///
/// 返回 `None` 表示原本有序（无需重排）；`Some(重排后 Vec)` 表示已恢复。
/// 同 tick 时未移动元素在前（等价有序插入的「稳定插到同 tick 之后」语义）。
pub fn restore_sorted_vec<T: EventTick + Clone>(all: &[T], moved: &[usize]) -> Option<Vec<T>> {
    let idxs = normalize_moved(moved, all.len());
    if idxs.is_empty() || all.len() <= 1 {
        return None;
    }
    if sorted_locally_in_slice(all, &idxs) {
        return None;
    }
    let mut rest: Vec<T> = Vec::with_capacity(all.len() - idxs.len());
    let mut displaced: Vec<T> = Vec::with_capacity(idxs.len());
    let mut mi = 0usize;
    for (i, e) in all.iter().enumerate() {
        if mi < idxs.len() && idxs[mi] == i {
            displaced.push(e.clone());
            mi += 1;
        } else {
            rest.push(e.clone());
        }
    }
    // 稳定排序：同 tick 的被移动元素保持原相对顺序
    displaced.sort_by_key(EventTick::tick);
    Some(merge_sorted(&rest, &displaced))
}

/// 归并两个已按 tick 升序的序列；同 tick 时 `a`（未移动元素）在前
fn merge_sorted<T: EventTick + Clone>(a: &[T], b: &[T]) -> Vec<T> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let (mut i, mut j) = (0usize, 0usize);
    while i < a.len() && j < b.len() {
        if a[i].tick() <= b[j].tick() {
            out.push(a[i].clone());
            i += 1;
        } else {
            out.push(b[j].clone());
            j += 1;
        }
    }
    out.extend(a[i..].iter().cloned());
    out.extend(b[j..].iter().cloned());
    out
}

impl<T: EventTick> ChunkedList<T> {
    /// 就地修改 `moved` 索引处元素的 tick 后，恢复「按 tick 升序」不变式。
    ///
    /// `moved` 为被修改元素的**修改前全局索引**（就地修改不移位，索引仍有效；
    /// 必须包含全部被修改元素）。返回 `true` 表示顺序确实发生变化（已重排）；
    /// `false` 表示原本有序（零分配空操作）。
    ///
    /// - 快路径：O(k log 块数) 局部邻域检查；
    /// - 重排：k ≤ [`SMALL_MOVED_THRESHOLD`] 时逐个 `remove` + `insert`
    ///   （O(k·块内)，无整表分配）；否则稳定归并重建（O(N + k log k)）。
    pub fn restore_sorted(&mut self, moved: &[usize]) -> bool
    where
        T: Clone,
    {
        let idxs = normalize_moved(moved, self.total_len);
        if idxs.is_empty() || self.total_len <= 1 {
            return false;
        }
        // 快路径：局部邻域检查（未移动元素相对顺序未变，逆序对必涉及被移动元素）
        let locally_sorted = idxs.iter().all(|&i| {
            let cur = self.get(i).map(EventTick::tick);
            let prev = i
                .checked_sub(1)
                .and_then(|p| self.get(p))
                .map(EventTick::tick);
            let next = self.get(i + 1).map(EventTick::tick);
            match (prev, cur, next) {
                (Some(p), Some(c), Some(n)) => p <= c && c <= n,
                (None, Some(c), Some(n)) => c <= n,
                (Some(p), Some(c), None) => p <= c,
                _ => true,
            }
        });
        if locally_sorted {
            return false;
        }
        if idxs.len() <= SMALL_MOVED_THRESHOLD {
            // 小集合：降序取出（避免索引漂移）后按 tick 有序插回
            let mut elems: Vec<T> = Vec::with_capacity(idxs.len());
            for &i in idxs.iter().rev() {
                if let Some(e) = self.remove(i) {
                    elems.push(e);
                }
            }
            for e in elems {
                self.insert(e);
            }
            return true;
        }
        // 大集合：稳定归并重建
        let mut rest: Vec<T> = Vec::with_capacity(self.total_len - idxs.len());
        let mut displaced: Vec<T> = Vec::with_capacity(idxs.len());
        let mut mi = 0usize;
        for (i, e) in self.iter().enumerate() {
            if mi < idxs.len() && idxs[mi] == i {
                displaced.push(e.clone());
                mi += 1;
            } else {
                rest.push(e.clone());
            }
        }
        displaced.sort_by_key(EventTick::tick);
        let merged = merge_sorted(&rest, &displaced);
        *self = Self::from_sorted(merged);
        true
    }
}
