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

/// 区间间隙合并阈值：间隙 ≤ 此值的受影区间合并为一个区间。
///
/// 合并的权衡：多一条消息 ≈ 数百音符的转换+上传成本，故近邻区间合并划算；
/// 但阈值过大会把「分散改动的稀疏区间」并成凸包、抵消稀疏收益——
/// 取 16：仅合并真正近邻的区间，稀疏场景保持稀疏（间隙 > 16 不合并）。
const RANGE_GAP_COALESCE: usize = 16;

/// 受影响区间数上限：超过则退化为单个凸包区间。
///
/// 极端分散改动（如散布全轨的选区拖动）下，「多区间」会退化为大量小消息
/// （渲染线程每条消息一次 `write_buffer`）——此时用单个凸包约束消息数，
/// 负载上限 = 当前音轨长度（**仍无全工程全量重建**）。
const MAX_REORDER_RANGES: usize = 512;

/// 重排受影响的索引区间集合（**升序、互不相交、闭区间**，最终索引空间）。
///
/// 契约：所有区间之外的内容与重排前逐位一致，调用方按各区间做增量更新
/// （GPU 段内 `UpdateRange`）即可，无需全量重建。
///
/// 正确性依据：重排是**排列**（元素数不变），每个被移动元素 e 的净位移被
/// 限制在其「旧位置 ∪ 新位置」凸包 `[min(o_e, n_e), max(o_e, n_e)]` 内；
/// 被位移的未移动元素必落在某个 moved 元素的凸包内（它被该元素跨过），
/// 故全部内容变化 ⊆ ∪_e 凸包(e)。同 tick 块内顺序可变，按块全宽保守放宽。
pub type SortedRestoreRanges = Vec<(usize, usize)>;

/// 凸包列表 → 合并（重叠/相邻/小间隙）+ 区间数上限
fn merge_hulls(mut hulls: Vec<(usize, usize)>) -> SortedRestoreRanges {
    if hulls.is_empty() {
        return Vec::new();
    }
    hulls.sort_unstable();
    let mut out: SortedRestoreRanges = Vec::with_capacity(hulls.len());
    for (lo, hi) in hulls {
        match out.last_mut() {
            Some(last) if lo <= last.1.saturating_add(RANGE_GAP_COALESCE + 1) => {
                last.1 = last.1.max(hi);
            }
            _ => out.push((lo, hi)),
        }
    }
    if out.len() > MAX_REORDER_RANGES {
        // 极端分散：退化为单凸包（约束消息数；负载 ≤ 当前音轨长度）
        let lo = out[0].0;
        let hi = out[out.len() - 1].1;
        return vec![(lo, hi)];
    }
    out
}

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
/// 返回 `None` 表示原本有序（无需重排）；`Some((重排后 Vec, 受影响区间))` 表示已恢复。
/// 同 tick 时未移动元素在前（等价有序插入的「稳定插到同 tick 之后」语义）。
pub fn restore_sorted_vec<T: EventTick + Clone>(
    all: &[T],
    moved: &[usize],
) -> Option<(Vec<T>, SortedRestoreRanges)> {
    let idxs = normalize_moved(moved, all.len());
    if idxs.is_empty() || all.len() <= 1 {
        return None;
    }
    if sorted_locally_in_slice(all, &idxs) {
        return None;
    }
    let mut rest: Vec<T> = Vec::with_capacity(all.len() - idxs.len());
    let mut displaced: Vec<(usize, T)> = Vec::with_capacity(idxs.len());
    let mut mi = 0usize;
    for (i, e) in all.iter().enumerate() {
        if mi < idxs.len() && idxs[mi] == i {
            displaced.push((i, e.clone()));
            mi += 1;
        } else {
            rest.push(e.clone());
        }
    }
    // 稳定排序：同 tick 的被移动元素保持原相对顺序
    displaced.sort_by_key(|(_, e)| EventTick::tick(e));
    let moved: Vec<(usize, u32)> = displaced
        .iter()
        .map(|(i, e)| (*i, EventTick::tick(e)))
        .collect();
    let merged = merge_sorted(
        &rest,
        &displaced.iter().map(|(_, e)| e.clone()).collect::<Vec<T>>(),
    );
    let ranges = ranges_in_sorted_slice(&merged, &moved);
    Some((merged, ranges))
}

/// 切片版受影响区间计算（与 [`ChunkedList::restore_sorted_ranges`] 同算法）。
fn ranges_in_sorted_slice<T: EventTick>(
    sorted: &[T],
    moved: &[(usize, u32)],
) -> SortedRestoreRanges {
    hulls_from_searches(moved, |t| {
        let lb = sorted.partition_point(|e| e.tick() < t);
        let ub = if t == u32::MAX {
            sorted.len()
        } else {
            sorted.partition_point(|e| e.tick() < t + 1)
        };
        (lb, ub)
    })
}

/// 受影响区间统一算法：`moved` 为 `(旧索引, 新 tick)` 对（**必须正确配对**）。
///
/// `search(t)` 返回 `(lower_bound, upper_bound)`——第一个 tick >= t 与第一个 tick > t。
/// 每个被移动元素贡献凸包 `[min(旧位置, 新位置块起), max(旧位置, 新位置块止)]`；
/// 同 tick 块内顺序可变 → 块全宽保守覆盖。
fn hulls_from_searches(
    moved: &[(usize, u32)],
    mut search: impl FnMut(u32) -> (usize, usize),
) -> SortedRestoreRanges {
    let mut hulls: Vec<(usize, usize)> = Vec::with_capacity(moved.len());
    for &(o, t) in moved {
        let (lb, ub) = search(t);
        let new_hi = ub.saturating_sub(1).max(lb);
        hulls.push((o.min(lb), o.max(new_hi)));
    }
    merge_hulls(hulls)
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
        self.restore_sorted_ranges(moved).is_some()
    }

    /// 同 [`Self::restore_sorted`]，但返回重排**受影响索引区间集合**
    /// （升序、互不相交、闭区间，最终索引空间）。
    ///
    /// 返回 `None` 表示原本有序（零重排）；`Some(ranges)` 表示已重排，调用方按各区间
    /// 做增量更新（GPU 段内 `UpdateRange`）——见 [`SortedRestoreRanges`] 契约
    /// （区间外内容逐位不变），**替代全量重建**。
    pub fn restore_sorted_ranges(&mut self, moved: &[usize]) -> Option<SortedRestoreRanges>
    where
        T: Clone,
    {
        let idxs = normalize_moved(moved, self.total_len);
        if idxs.is_empty() || self.total_len <= 1 {
            return None;
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
            return None;
        }
        // (旧索引, 新 tick) 配对：小集合路径取出为降序、归并路径按 tick 排序，
        // 均需还原为旧索引升序后与 `idxs` 一一对应（配对错误会算错凸包）。
        let mut moved: Vec<(usize, u32)> = Vec::with_capacity(idxs.len());
        if idxs.len() <= SMALL_MOVED_THRESHOLD {
            // 小集合：降序取出（避免索引漂移）后按 tick 有序插回
            let mut elems: Vec<(usize, T)> = Vec::with_capacity(idxs.len());
            for &i in idxs.iter().rev() {
                if let Some(e) = self.remove(i) {
                    elems.push((i, e));
                }
            }
            // 降序取出 → 反转为旧索引升序
            moved.extend(elems.iter().rev().map(|(i, e)| (*i, EventTick::tick(e))));
            for (_, e) in elems {
                self.insert(e);
            }
        } else {
            // 大集合：稳定归并重建
            let mut rest: Vec<T> = Vec::with_capacity(self.total_len - idxs.len());
            let mut displaced: Vec<(usize, T)> = Vec::with_capacity(idxs.len());
            let mut mi = 0usize;
            for (i, e) in self.iter().enumerate() {
                if mi < idxs.len() && idxs[mi] == i {
                    displaced.push((i, e.clone()));
                    mi += 1;
                } else {
                    rest.push(e.clone());
                }
            }
            moved.extend(displaced.iter().map(|(i, e)| (*i, EventTick::tick(e))));
            // 稳定排序：同 tick 的被移动元素保持原相对顺序
            displaced.sort_by_key(|(_, e)| EventTick::tick(e));
            let displaced_elems: Vec<T> = displaced.into_iter().map(|(_, e)| e).collect();
            let merged = merge_sorted(&rest, &displaced_elems);
            *self = Self::from_sorted(merged);
        }
        Some(hulls_from_searches(&moved, |t| {
            let lb = self.partition_point(t);
            let ub = if t == u32::MAX {
                self.total_len
            } else {
                self.partition_point(t + 1)
            };
            (lb, ub)
        }))
    }
}
