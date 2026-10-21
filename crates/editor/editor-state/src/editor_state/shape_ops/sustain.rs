//! Sustain（堆叠延音）音符算法，DRAW-008
//!
//! 对应上游 `scripts/notes/sustain.py`：同 key 行上 gap 不超过 `hold_gap` 的音符归为一组，
//! 组内每条延长到组末（或 `most` 上限），形成「声部墙」；gap 更大的空洞保持为空。
//! 可选 stop 在指定 tick 把跨过的音符切开，并在同 tick 每 key 重新起一条（无缝隙）。
//!
//! 位置：Notes 阶段**最后**一步，只改长度。`auto_gap` 与 DRAW-009 共用同一实现。

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::collections::btree_map::Entry;

use lumino_core::shape::SustainParams;

use crate::ShapeNote;

use super::gateflip::{auto_gap, row_order};

const EPS: f32 = 1e-6;
const MAX_PIECES: usize = 20_000_000;
const MAX_STOPS: usize = 1_000_000;

/// Sustain 执行报告
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SustainReport {
    /// 实际使用的 stop 数
    pub stops: usize,
    /// 实际产出的音符数（放弃切分时为延长后的音符数）
    pub pieces: usize,
    /// 是否因 stop/切片超限而放弃 stop 切分（只做延长）
    pub gave_up: bool,
}

/// 应用 Sustain；`ppq` 用于拍 → tick
pub fn apply(
    notes: &[ShapeNote],
    params: &SustainParams,
    ppq: f64,
) -> (Vec<ShapeNote>, SustainReport) {
    apply_with_limits(notes, params, ppq, MAX_PIECES, MAX_STOPS)
}

fn apply_with_limits(
    notes: &[ShapeNote],
    params: &SustainParams,
    ppq: f64,
    max_pieces: usize,
    max_stops: usize,
) -> (Vec<ShapeNote>, SustainReport) {
    if notes.is_empty() || !ppq.is_finite() || ppq <= 0.0 {
        return (notes.to_vec(), SustainReport::default());
    }
    let hold_gap = match params.gap_beats {
        Some(beats) => (beats as f64 * ppq).round().max(0.0) as f32,
        None => auto_gap(notes),
    };
    let (stops, too_many_stops) = stop_ticks(notes, params, ppq, max_stops);
    let max_end = notes
        .iter()
        .map(|note| note.tick + note.length.max(0.0))
        .fold(f32::NEG_INFINITY, f32::max);
    let reach = stops.last().copied().filter(|&stop| stop > max_end);
    let most_ticks = params
        .most_beats
        .map(|beats| (beats as f64 * ppq).round().max(1.0) as f32);
    let extended = sustained(notes, hold_gap, most_ticks, reach);

    if too_many_stops {
        let pieces = extended.len();
        return (
            extended,
            SustainReport {
                stops: 0,
                pieces,
                gave_up: true,
            },
        );
    }
    if stops.is_empty() {
        return (extended, SustainReport::default());
    }
    let estimate = estimated_pieces(&extended, &stops);
    if estimate > max_pieces {
        return (
            extended,
            SustainReport {
                stops: stops.len(),
                pieces: estimate,
                gave_up: true,
            },
        );
    }
    let cut = cut_at(&extended, &stops);
    let pieces = cut.len();
    (
        cut,
        SustainReport {
            stops: stops.len(),
            pieces,
            gave_up: false,
        },
    )
}

/// 按 key 行分组并延长；`reach` 只作用于每个 key 行的最后一组
fn sustained(
    notes: &[ShapeNote],
    hold_gap: f32,
    most_ticks: Option<f32>,
    reach: Option<f32>,
) -> Vec<ShapeNote> {
    if notes.is_empty() {
        return Vec::new();
    }
    let (order, gaps) = row_order(notes);
    let mut group = vec![0usize; order.len()];
    let mut current = 0usize;
    for (position, gap) in gaps.iter().enumerate() {
        if position > 0 && (*gap < 0.0 || *gap > hold_gap) {
            current += 1;
        }
        group[position] = current;
    }
    let mut group_end = vec![f32::NEG_INFINITY; current + 1];
    for (position, &index) in order.iter().enumerate() {
        let end = notes[index].tick + notes[index].length.max(0.0);
        group_end[group[position]] = group_end[group[position]].max(end);
    }
    if let Some(reach) = reach {
        let mut position = 0;
        while position < order.len() {
            let key = notes[order[position]].key;
            let mut row_end = position + 1;
            while row_end < order.len() && notes[order[row_end]].key == key {
                row_end += 1;
            }
            let last_group = group[row_end - 1];
            group_end[last_group] = group_end[last_group].max(reach);
            position = row_end;
        }
    }

    let mut out = notes.to_vec();
    for (position, &index) in order.iter().enumerate() {
        let note = notes[index];
        let original_end = note.tick + note.length.max(0.0);
        let mut end = group_end[group[position]].max(original_end);
        if let Some(most) = most_ticks {
            end = end.min(note.tick + most).max(original_end);
        }
        out[index].length = (end - note.tick).max(0.0);
    }
    out
}

/// 生成 stop tick（在**延长前**的音符区间上算），并报告是否超过 stop 上限
fn stop_ticks(
    notes: &[ShapeNote],
    params: &SustainParams,
    ppq: f64,
    max_stops: usize,
) -> (Vec<f32>, bool) {
    if notes.is_empty() {
        return (Vec::new(), false);
    }
    let lo = notes
        .iter()
        .map(|note| note.tick)
        .fold(f32::INFINITY, f32::min);
    let hi = notes
        .iter()
        .map(|note| note.tick + note.length.max(0.0))
        .fold(f32::NEG_INFINITY, f32::max);
    let mut stops = Vec::new();

    if let Some(every) = params.every_beats {
        let step = (every as f64 * ppq).round().max(1.0);
        let from = params.from_beats.unwrap_or(0.0) as f64;
        let first = lo as f64 + (from * ppq).round();
        let k0 = (((lo as f64 - first) / step).floor() + 1.0).max(0.0);
        let last = (((hi as f64 - 1.0 - first) / step).floor() + 1.0).max(0.0);
        let wanted = (last - k0).max(0.0);
        if wanted > max_stops as f64 {
            return (Vec::new(), true);
        }
        for index in 0..wanted as usize {
            stops.push((first + step * (k0 + index as f64)) as f32);
        }
    }

    if let Some(manual) = &params.stops
        && !manual.off
    {
        for &share in &manual.at {
            let at = lo + (share as f64 * (hi - lo) as f64).round() as f32;
            if at > lo && (at < hi || manual.past) {
                stops.push(at);
            }
        }
    }

    stops.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    stops.dedup_by(|a, b| (*a - *b).abs() <= EPS);
    (stops, false)
}

/// 每个 stop 切一刀，再按「同 key、同起点」合并 stop 处的新音符
fn cut_at(notes: &[ShapeNote], stops: &[f32]) -> Vec<ShapeNote> {
    if notes.is_empty() || stops.is_empty() {
        return notes.to_vec();
    }
    let mut ordinary = Vec::new();
    let mut merged: BTreeMap<(u16, i64), ShapeNote> = BTreeMap::new();
    for note in notes {
        let start = note.tick;
        let end = start + note.length.max(0.0);
        if end <= start + EPS {
            continue;
        }
        let from = stops.partition_point(|stop| *stop <= start + EPS);
        let mut cursor = start;
        let mut at = at_stop(stops, start);
        for &stop in &stops[from..] {
            if stop >= end - EPS {
                break;
            }
            push_piece(*note, cursor, stop, at, &mut ordinary, &mut merged);
            cursor = stop;
            at = true;
        }
        push_piece(*note, cursor, end, at, &mut ordinary, &mut merged);
    }
    ordinary.extend(merged.into_values());
    ordinary
}

fn push_piece(
    note: ShapeNote,
    start: f32,
    end: f32,
    at_stop: bool,
    ordinary: &mut Vec<ShapeNote>,
    merged: &mut BTreeMap<(u16, i64), ShapeNote>,
) {
    if end <= start + EPS {
        return;
    }
    if !at_stop {
        ordinary.push(ShapeNote {
            tick: start,
            length: end - start,
            ..note
        });
        return;
    }
    let key = (note.key, start.round() as i64);
    match merged.entry(key) {
        Entry::Vacant(slot) => {
            slot.insert(ShapeNote {
                tick: start,
                length: end - start,
                ..note
            });
        }
        Entry::Occupied(mut slot) => {
            let existing = slot.get_mut();
            let old_end = existing.tick + existing.length;
            if note.velocity > existing.velocity
                || (note.velocity == existing.velocity && end > old_end)
            {
                existing.track = note.track;
                existing.velocity = note.velocity;
            }
            let new_end = old_end.max(end);
            existing.length = new_end - existing.tick;
        }
    }
}

fn at_stop(stops: &[f32], tick: f32) -> bool {
    let index = stops.partition_point(|stop| *stop < tick - EPS);
    index < stops.len() && (stops[index] - tick).abs() <= EPS
}

fn estimated_pieces(notes: &[ShapeNote], stops: &[f32]) -> usize {
    let mut total = notes.len();
    for note in notes {
        let start = note.tick;
        let end = start + note.length.max(0.0);
        let from = stops.partition_point(|stop| *stop <= start + EPS);
        for &stop in &stops[from..] {
            if stop >= end - EPS {
                break;
            }
            total = total.saturating_add(1);
        }
    }
    total
}

#[cfg(test)]
mod tests;
