//! 颜料桶填充的**逐音高行区间**几何：蜘蛛网（Spiderweb）式封闭图形内部
//!
//! 移植自 Spiderweb `scripts/notes/custom.py` 的 `poly_edges` / `row_spans` /
//! `merge_spans`，替换原先「按 snap 网格枚举格点、每格一个定长音符」的做法：
//!
//! - 对每条音高行 `q`，解析求出闭环内部与该行（底边 `q - 0.5`、顶边 `q + 0.5`）
//!   相交的全部时间区间；区间端点 = 闭环边与行上下边界的**解析交点**，
//!   精确到 tick、不做任何网格量化，每个区间一条音符；
//! - even-odd 规则（第 1 与第 2、第 3 与第 4 个交点配对）→ 嵌套洞保持空；
//! - 多环区域身份：把该行全部闭环区间端点当切分点切成**原子区间**，每个原子
//!   区间取「被哪些闭环覆盖」的位掩码；掩码属于用户点击标记的区域集合时填充
//!   → 多个独立图形、嵌套洞、背景蔓延的语义与原实现完全一致。

use super::loops::loop_covers_cell;
use crate::interaction::line_tool::paths::RawNote;
use std::cmp::Ordering;
use std::collections::HashSet;

/// 区域身份位掩码（位 `i` = 被第 `i` 个闭环覆盖）；`0` = 背景
pub(crate) type RegionKey = u128;
/// 参与区域身份计算的闭环上限（位掩码宽度）
const MAX_REGION_LOOPS: usize = 128;

fn asc(a: &f32, b: &f32) -> Ordering {
    a.partial_cmp(b).unwrap_or(Ordering::Equal)
}

/// 闭环多边形与音高行 `q` 内部相交的全部时间区间（逻辑 tick），even-odd 规则。
///
/// 多边形点序须首尾重复（[`super::loops::assemble_loops`] 的输出形态）。
/// 水平边跨不了音高行，直接跳过；相邻两个「层」（行边界 + 落在行内的拐点）
/// 之间没有拐点，所以每条边在那里都是直的一段，每对交点恰好构成一个梯形。
pub(crate) fn row_spans(poly: &[(f32, f32)], q: f32) -> Vec<(f32, f32)> {
    let (lo, hi) = (q - 0.5, q + 0.5);
    let mut edges: Vec<(f32, f32, f32, f32)> = Vec::new();
    for w in poly.windows(2) {
        let (a, b) = (w[0], w[1]);
        if a.1 == b.1 {
            continue;
        }
        if a.1.min(b.1) < hi && a.1.max(b.1) > lo {
            edges.push((a.0, a.1, b.0, b.1));
        }
    }
    if edges.is_empty() {
        return Vec::new();
    }
    // 层 = 行上下边界 + 严格落在行内的拐点
    let mut levels: Vec<f32> = vec![lo, hi];
    for e in &edges {
        if e.1 > lo && e.1 < hi {
            levels.push(e.1);
        }
        if e.3 > lo && e.3 < hi {
            levels.push(e.3);
        }
    }
    levels.sort_by(asc);
    levels.dedup();
    let level_at = |v: f32| levels.partition_point(|&l| l < v);
    let x_at = |e: &(f32, f32, f32, f32), y: f32| e.0 + (e.2 - e.0) * (y - e.1) / (e.3 - e.1);

    // 每条边在每个它穿过的层隙里各出现一次（逐边展开 → 层隙内保持左右次序）
    let mut entries: Vec<(usize, f32, f32, f32)> = Vec::new();
    for e in &edges {
        let from = level_at(e.1.min(e.3).max(lo));
        let to = level_at(e.1.max(e.3).min(hi));
        for g in from..to {
            let (y0, y1) = (levels[g], levels[g + 1]);
            let (xa, xb) = (x_at(e, y0), x_at(e, y1));
            entries.push((g, (xa + xb) * 0.5, xa.min(xb), xa.max(xb)));
        }
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0).then(asc(&a.1, &b.1)));

    // 每个层隙内从左到右：第 1 与第 2、第 3 与第 4 … 配对（even-odd）
    let mut spans: Vec<(f32, f32)> = Vec::new();
    let mut i = 0;
    while i < entries.len() {
        let gap = entries[i].0;
        let mut end = i;
        while end < entries.len() && entries[end].0 == gap {
            end += 1;
        }
        let mut k = i;
        while k + 1 < end {
            spans.push((entries[k].2, entries[k + 1].3));
            k += 2;
        }
        i = end;
    }
    merge_spans(&spans)
}

/// 区间合并：按起点排序，相接（含端点相等）或重叠的合成一个。
fn merge_spans(spans: &[(f32, f32)]) -> Vec<(f32, f32)> {
    if spans.is_empty() {
        return Vec::new();
    }
    let mut items = spans.to_vec();
    items.sort_by(|a, b| asc(&a.0, &b.0));
    let mut out: Vec<(f32, f32)> = Vec::new();
    let mut cur = items[0];
    for &(a, b) in &items[1..] {
        if a > cur.1 {
            out.push(cur);
            cur = (a, b);
        } else {
            cur.1 = cur.1.max(b);
        }
    }
    out.push(cur);
    out
}

/// 点的区域身份（位 `i` = 被第 `i` 个闭环**覆盖**）。
///
/// 与渲染层同一条 [`loop_covers_cell`] 规则（中心在环内 ∨ 距边 < 半格）
/// → 生成的音符覆盖 = 填充显示区域，边框一致。
fn region_key(loops: &[Vec<(f32, f32)>], cx: f32, cy: f32, snap: f32) -> RegionKey {
    let mut key = 0u128;
    for (i, lp) in loops.iter().enumerate().take(MAX_REGION_LOOPS) {
        if loop_covers_cell(lp, cx, cy, snap) {
            key |= 1u128 << i;
        }
    }
    key
}

/// 每个填充标记的**区域身份**（去重后的集合）。
///
/// 标记记录的是「用户点的那一格」（吸附 tick + 整数 key），故取该格中心
/// 作为几何探针：`snap` 在这里只用于还原**输入位置**，不参与音符边界量化。
pub(crate) fn mark_regions(
    loops: &[Vec<(f32, f32)>],
    marks: &[(f32, u16)],
    snap: f32,
) -> HashSet<RegionKey> {
    marks
        .iter()
        .map(|&(t, k)| region_key(loops, t + snap * 0.5, k as f32 + 0.5, snap))
        .collect()
}

/// 切分步长（tick）：x 分音符 = 4·ppq / x（与 `NotePrecision::as_ticks` 同口径，
/// 四分音符 = ppq）。取整到整 tick——x 取非 2 的幂（3/5/7…）时步长非整数，
/// 不取整会让切点逐格累积漂移。
pub(crate) fn division_step(division: u32, ppq: u16) -> f32 {
    let ppq = ppq.max(1) as f32;
    let x = division.max(1) as f32;
    (4.0 * ppq / x).round().max(1.0)
}

/// 把区间 `[a, b]` 按**全局网格**（步长 `step`，从 tick 0 起算）切成若干段。
///
/// 语义 = 区间与每个网格单元 `[k·step, (k+1)·step]` 求交：内部切点全部落在
/// 全局网格线上（与标尺/吸附一致），首尾不足一份的残段**保留**为短音符，
/// 因此切分后的总覆盖严格等于原区间（填充不出现空洞 → 预览与音符一致）。
pub(crate) fn chop_span(a: f32, b: f32, step: f32) -> Vec<(f32, f32)> {
    if b <= a || step <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut t = a;
    while t < b {
        let cell_end = ((t / step).floor() + 1.0) * step;
        let e = cell_end.min(b);
        out.push((t, e));
        t = e;
    }
    out
}

/// 逐音高行生成填充音符（tick 精确、无网格量化）。
///
/// 对每行 `q`：取该行全部闭环区间端点（钳到 `[tick_lo, tick_hi]`）作为切分点，
/// 切成原子区间；原子区间的覆盖掩码 ∈ `regions` 时填充，相邻的合并。
/// 覆盖掩码为 `0` 的原子区间 = 背景（`regions` 含 `0` 时蔓延）。
///
/// `snap` 不参与：区间端点就是几何交点。
/// `division` = 填充桶的切分档位（`Some(x)` → 每行区间再按 x 分音符的全局
/// 网格切分；`None` = 每个区间一条长音符）。
pub(crate) fn fill_spans(
    loops: &[Vec<(f32, f32)>],
    regions: &HashSet<RegionKey>,
    tick_lo: f32,
    tick_hi: f32,
    key_lo: i32,
    key_hi: i32,
    division: Option<u32>,
    ppq: u16,
) -> Vec<RawNote> {
    if regions.is_empty() || tick_hi <= tick_lo {
        return Vec::new();
    }
    let step = division.map(|d| division_step(d, ppq));
    let mut out: Vec<RawNote> = Vec::new();
    for q in key_lo..=key_hi {
        let per_loop: Vec<Vec<(f32, f32)>> =
            loops.iter().map(|lp| row_spans(lp, q as f32)).collect();
        let mut bounds: Vec<f32> = vec![tick_lo, tick_hi];
        for sp in &per_loop {
            for &(a, b) in sp {
                bounds.push(a.clamp(tick_lo, tick_hi));
                bounds.push(b.clamp(tick_lo, tick_hi));
            }
        }
        bounds.sort_by(asc);
        bounds.dedup();

        let mut spans: Vec<(f32, f32)> = Vec::new();
        for w in bounds.windows(2) {
            let (x0, x1) = (w[0], w[1]);
            if x1 <= x0 {
                continue;
            }
            let mid = (x0 + x1) * 0.5;
            let mut key: RegionKey = 0;
            for (i, sp) in per_loop.iter().enumerate().take(MAX_REGION_LOOPS) {
                if sp.iter().any(|&(a, b)| a <= mid && mid <= b) {
                    key |= 1u128 << i;
                }
            }
            if !regions.contains(&key) {
                continue;
            }
            match spans.last_mut() {
                Some(prev) if prev.1 >= x0 => prev.1 = x1,
                _ => spans.push((x0, x1)),
            }
        }

        for (a, b) in spans {
            // 切分档位开启：按 x 分音符的全局网格再切一刀（残段保留 → 覆盖不变）
            let pieces = match step {
                Some(s) => chop_span(a, b, s),
                None => vec![(a, b)],
            };
            for (a, b) in pieces {
                let start = ((a + 0.5).floor() as i64).max(0);
                let end = ((b + 0.5).floor() as i64).max(start + 1);
                out.push(RawNote { start, end, key: q });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
