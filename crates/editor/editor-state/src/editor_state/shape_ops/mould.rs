//! Mould 饼干模（DRAW-025）
//!
//! 把同一 key 行上相邻墙音符之间的空隙，按其中点是否在轮廓内筛出来。
//! 短于 `SHORT × 相邻音符长度` 的碎片丢弃。

use std::collections::BTreeMap;

use crate::ShapeNote;

use super::slice::{Pt, inside};

/// 被切掉的碎片短于音符长度的此比例则丢弃
pub const SHORT: f32 = 0.25;

/// 每 key 行：墙音符之间、且空隙中点在轮廓内的区间
pub fn mould_rows(walls: &[ShapeNote], loops: &[Vec<Pt>], _ppq: u32) -> Vec<(u16, f32, f32)> {
    let mut by_key: BTreeMap<u16, Vec<(f32, f32)>> = BTreeMap::new();
    for wall in walls {
        if wall.length <= 0.0 || !wall.tick.is_finite() || !wall.length.is_finite() {
            continue;
        }
        by_key
            .entry(wall.key)
            .or_default()
            .push((wall.tick, wall.tick + wall.length));
    }
    let mut out = Vec::new();
    for (key, mut spans) in by_key {
        spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        for pair in spans.windows(2) {
            let start = pair[0].1;
            let end = pair[1].0;
            if end <= start {
                continue;
            }
            let min_length = pair[0].1 - pair[0].0;
            if end - start < SHORT * min_length {
                continue;
            }
            let mid = (start + end) / 2.0;
            if inside(loops, (mid, key as f32 + 0.5)) {
                out.push((key, start, end));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    out
}

#[cfg(test)]
mod tests;
