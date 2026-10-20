//! Claw machine 音符算法
//!
//! 与上游 `notes/claw.py` 对齐的纯音符后处理：
//! - **time**：按 `period` 分段，`trash=(k,n)` 丢掉每段第 k/n 片；`close` 压掉丢弃片，
//!   `stretch` 拉回原总长，`dist` 段内指数弯曲，`short` 删除新短音；
//! - **notes / keys / chords**：`n % (keep + skip) < keep`；
//! - **random**：稳定排序 + seed 伪随机保留 `pct%`；
//! - 计数/random 模式下 `shorten` 把丢弃音符改短为 `cut%`。

use std::collections::BTreeSet;

use lumino_core::shape::{ClawMode, ClawParams};

use crate::ShapeNote;

const EPS: f64 = 1e-6;

/// 应用 Claw；`ppq` 仅 time 模式需要（拍 → tick）
pub fn apply(notes: &[ShapeNote], params: &ClawParams, ppq: f64) -> Vec<ShapeNote> {
    if notes.is_empty() {
        return Vec::new();
    }
    match params.mode {
        ClawMode::Time => {
            if !ppq.is_finite() || ppq <= 0.0 {
                return notes.to_vec();
            }
            time_mode(notes, params, ppq)
        }
        ClawMode::Notes => count_mode(notes, params, CountBy::Notes),
        ClawMode::Keys => count_mode(notes, params, CountBy::Keys),
        ClawMode::Chords => count_mode(notes, params, CountBy::Chords),
        ClawMode::Random => random_mode(notes, params),
    }
}

#[derive(Clone, Copy)]
enum CountBy {
    Notes,
    Keys,
    Chords,
}

fn count_mode(notes: &[ShapeNote], params: &ClawParams, by: CountBy) -> Vec<ShapeNote> {
    let cycle = (params.keep as usize) + (params.skip as usize);
    if cycle == 0 {
        return notes.to_vec();
    }
    let keep_n = params.keep as usize;
    let mut keep = vec![false; notes.len()];
    match by {
        CountBy::Notes => {
            let mut order = sorted_indexed(notes);
            for (position, (index, _)) in order.drain(..).enumerate() {
                keep[index] = position % cycle < keep_n;
            }
        }
        CountBy::Keys => {
            let Some(min_key) = notes.iter().map(|note| note.key).min() else {
                return Vec::new();
            };
            for (index, note) in notes.iter().enumerate() {
                let key_index = (note.key - min_key) as usize;
                keep[index] = key_index % cycle < keep_n;
            }
        }
        CountBy::Chords => {
            let mut order = sorted_indexed(notes);
            let mut chord = 0usize;
            let mut last_tick: Option<f32> = None;
            for (index, note) in order.drain(..) {
                if last_tick.is_some_and(|previous| (previous - note.tick).abs() > f32::EPSILON) {
                    chord += 1;
                }
                last_tick = Some(note.tick);
                keep[index] = chord % cycle < keep_n;
            }
        }
    }
    apply_keep(notes, &keep, params)
}

fn random_mode(notes: &[ShapeNote], params: &ClawParams) -> Vec<ShapeNote> {
    let mut keep = vec![false; notes.len()];
    let mut state = params.seed | 1;
    for (index, _) in sorted_indexed(notes) {
        let roll = next_rand(&mut state) % 100;
        keep[index] = roll < params.pct as u32;
    }
    apply_keep(notes, &keep, params)
}

fn sorted_indexed(notes: &[ShapeNote]) -> Vec<(usize, ShapeNote)> {
    let mut order: Vec<(usize, ShapeNote)> = notes.iter().copied().enumerate().collect();
    order.sort_by(|(ia, a), (ib, b)| {
        a.tick
            .partial_cmp(&b.tick)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.key.cmp(&b.key))
            .then(ia.cmp(ib))
    });
    order
}

fn apply_keep(notes: &[ShapeNote], keep: &[bool], params: &ClawParams) -> Vec<ShapeNote> {
    let mut out = Vec::with_capacity(notes.len());
    for (index, note) in notes.iter().enumerate() {
        if keep[index] {
            out.push(*note);
        } else if params.shorten {
            let length = (note.length.max(0.0) * params.cut as f32 / 100.0).max(1.0);
            out.push(ShapeNote { length, ..*note });
        }
    }
    out
}

fn next_rand(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

struct Working {
    original: ShapeNote,
    start: f64,
    end: f64,
}

fn time_mode(notes: &[ShapeNote], params: &ClawParams, ppq: f64) -> Vec<ShapeNote> {
    let t0 = notes
        .iter()
        .map(|note| note.tick as f64)
        .fold(f64::INFINITY, f64::min);
    let period = params.period.beats() as f64 * ppq;
    if !period.is_finite() || period <= EPS {
        return notes.to_vec();
    }
    let Some(trash) = params.trash else {
        // None = 整段丢弃
        return Vec::new();
    };
    let n = trash.n.max(1) as f64;
    let k = trash.k as f64;
    if k >= n {
        return notes.to_vec();
    }
    let sub = period / n;
    let last_end = notes
        .iter()
        .map(|note| (note.tick + note.length.max(0.0)) as f64)
        .fold(t0, f64::max);

    // 只对「有音符起点」的段制造丢弃片，避免空段带来无意义位移；
    // 用集合而不是「总段数」数组，避免长工程按 period 分配巨量内存。
    let mut segments = BTreeSet::new();
    for note in notes {
        let segment = (((note.tick as f64 - t0) / period).floor() as isize).max(0) as usize;
        segments.insert(segment);
    }
    let mut intervals = Vec::with_capacity(segments.len());
    for segment in segments {
        let start = t0 + segment as f64 * period + k * sub;
        intervals.push((start, start + sub));
    }
    if intervals.is_empty() {
        return notes.to_vec();
    }
    let mut prefix = Vec::with_capacity(intervals.len() + 1);
    prefix.push(0.0);
    for &(start, end) in &intervals {
        let previous = prefix.last().copied().unwrap_or(0.0);
        prefix.push(previous + (end - start));
    }
    let dropped_before = |time: f64| -> f64 {
        let index = intervals.partition_point(|(_, end)| *end <= time + EPS);
        prefix[index]
    };

    let mut working = Vec::with_capacity(notes.len());
    for note in notes {
        let start = note.tick as f64;
        let mut end = start + note.length.max(0.0) as f64;
        let index = intervals.partition_point(|(_, interval_end)| *interval_end <= start + EPS);
        if let Some(&(interval_start, _)) = intervals.get(index) {
            if start >= interval_start - EPS {
                continue; // 起点落在丢弃片
            }
            if end > interval_start + EPS {
                end = interval_start; // 跨过丢弃片：截断在片首
            }
        }
        if end <= start + EPS {
            continue;
        }
        let new_start = start - dropped_before(start);
        let new_end = end - dropped_before(end);
        if new_end <= new_start + EPS {
            continue;
        }
        working.push(Working {
            original: *note,
            start: new_start,
            end: new_end,
        });
    }
    if working.is_empty() {
        return Vec::new();
    }

    let mut scale = 1.0;
    if params.stretch {
        let original_span = last_end - t0;
        let new_last = working.iter().map(|item| item.end).fold(t0, f64::max);
        let new_span = new_last - t0;
        if original_span > EPS && new_span > EPS {
            scale = original_span / new_span;
            for item in &mut working {
                item.start = t0 + (item.start - t0) * scale;
                item.end = t0 + (item.end - t0) * scale;
            }
        }
    }

    if params.dist != 0 {
        let period_scaled = period * scale;
        if period_scaled > EPS {
            let power = 1.0 + params.dist as f64 / 100.0;
            for item in &mut working {
                let segment = ((item.start - t0) / period_scaled).floor().max(0.0);
                let segment_start = t0 + segment * period_scaled;
                let warp = |time: f64| {
                    let u = ((time - segment_start) / period_scaled).clamp(0.0, 1.0);
                    let bent = if power > 1.0 {
                        1.0 - (1.0 - u).powf(power)
                    } else {
                        u.powf(power)
                    };
                    segment_start + bent * period_scaled
                };
                let start = warp(item.start);
                let end = warp(item.end);
                item.start = start;
                item.end = end.max(start);
            }
        }
    }

    let short_ticks = ppq / 24.0;
    let mut out = Vec::with_capacity(working.len());
    for item in working {
        let length = item.end - item.start;
        if length <= EPS {
            continue;
        }
        let original = item.original.length.max(0.0) as f64;
        if params.short && length < short_ticks && original >= short_ticks {
            continue;
        }
        out.push(ShapeNote {
            track: item.original.track,
            tick: item.start as f32,
            key: item.original.key,
            length: length as f32,
            velocity: item.original.velocity,
        });
    }
    out
}

#[cfg(test)]
mod tests;
