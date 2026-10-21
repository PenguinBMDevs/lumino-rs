//! Gate sensitive merge（DRAW-022）
//!
//! stay 不动；slide 的每个 key 行各自在时间上滑动，直到与 stay 同 key 行相接
//! （slide end = stay start，或 slide start = stay end），长度保持不变。

use std::collections::BTreeMap;

use lumino_core::shape::MergeParams;

use crate::ShapeNote;

/// 合并结果（滑移量，单位 tick）
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MergeOutcome {
    /// 接上的 key 行
    pub met_keys: Vec<u16>,
    /// 最小绝对滑移量（带符号）
    pub min_slide: i64,
    /// 为避免负 tick 的整体后移
    pub shift_all: i64,
    /// 每行滑移量（含 leftover）
    pub by_key: BTreeMap<u16, i64>,
}

/// slide 中点是否在 stay 中点之前
pub fn comes_from_left(stay: &[ShapeNote], slide: &[ShapeNote]) -> bool {
    midpoint(slide) < midpoint(stay)
}

/// 逐 key 行门控合并；无一行接上返回 `None`
pub fn gate_merge(
    stay: &[ShapeNote],
    slide: &[ShapeNote],
    params: &MergeParams,
) -> Option<MergeOutcome> {
    let stay_by_key = group(stay);
    let slide_by_key = group(slide);
    if stay_by_key.is_empty() || slide_by_key.is_empty() {
        return None;
    }
    let from_left = params
        .from_left
        .unwrap_or_else(|| comes_from_left(stay, slide));
    let mut by_key = BTreeMap::new();
    for (key, notes) in &slide_by_key {
        let Some(stay_notes) = stay_by_key.get(key) else {
            continue;
        };
        let Some((slide_start, slide_end)) = interval(notes) else {
            continue;
        };
        let Some((stay_start, stay_end)) = interval(stay_notes) else {
            continue;
        };
        let shift = if from_left {
            stay_start - slide_end
        } else {
            stay_end - slide_start
        };
        by_key.insert(*key, shift);
    }
    if by_key.is_empty() {
        return None;
    }
    let min_slide = by_key
        .values()
        .copied()
        .min_by_key(|value| value.abs())
        .unwrap_or(0);
    let fallback = if params.rest_slides { min_slide } else { 0 };
    for key in slide_by_key.keys() {
        by_key.entry(*key).or_insert(fallback);
    }
    let mut min_tick = i64::MAX;
    for (key, notes) in &slide_by_key {
        let shift = by_key.get(key).copied().unwrap_or(0);
        for note in notes {
            min_tick = min_tick.min(round(note.tick) + shift);
        }
    }
    let shift_all = if min_tick < 0 { -min_tick } else { 0 };
    Some(MergeOutcome {
        met_keys: by_key
            .keys()
            .filter(|key| stay_by_key.contains_key(key))
            .copied()
            .collect(),
        min_slide,
        shift_all,
        by_key,
    })
}

fn group(notes: &[ShapeNote]) -> BTreeMap<u16, Vec<&ShapeNote>> {
    let mut out: BTreeMap<u16, Vec<&ShapeNote>> = BTreeMap::new();
    for note in notes {
        out.entry(note.key).or_default().push(note);
    }
    out
}

fn interval(notes: &[&ShapeNote]) -> Option<(i64, i64)> {
    let start = notes.iter().map(|note| round(note.tick)).min()?;
    let end = notes
        .iter()
        .map(|note| round(note.tick + note.length))
        .max()?;
    (end > start).then_some((start, end))
}

fn midpoint(notes: &[ShapeNote]) -> i64 {
    notes
        .iter()
        .map(|note| round(note.tick + note.length / 2.0))
        .sum::<i64>()
        / notes.len().max(1) as i64
}

fn round(value: f32) -> i64 {
    value.round() as i64
}

#[cfg(test)]
mod tests;
