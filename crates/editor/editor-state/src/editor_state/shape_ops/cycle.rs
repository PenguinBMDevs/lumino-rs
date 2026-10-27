//! Colours 通道循环算法（DRAW-012）
//!
//! 对应 Spiderweb `custom.py:2962` 的 `cycle_turns`：按 step/key/time 轮转通道，
//! 支持每 key 行独立起算（First/End）与对角 `[s,m]`。只改 `channel`，不动其它字段。

use std::collections::BTreeMap;

use lumino_core::shape::{AREA_LOCK_BASE, CycleBy, CycleParams, RowRestart};

use crate::ShapeNote;

const EPS: f32 = 1e-4;

/// 应用通道循环；返回处理的音符数
pub fn apply(notes: &mut [ShapeNote], params: &CycleParams, ppq: f64) -> usize {
    if notes.is_empty() || !(2..=16).contains(&params.n) {
        return 0;
    }
    let n = i64::from(params.n);
    let lowest = notes
        .iter()
        .filter(|note| note.channel < AREA_LOCK_BASE)
        .map(|note| note.key)
        .min()
        .unwrap_or(0);
    let every = i64::from(params.every.max(1));
    let shift = params
        .shift
        .map(|[s, m]| (i64::from(s), i64::from(m.max(1))));

    match params.by {
        CycleBy::Step => {
            let rows = step_rows(notes, params.rows);
            for note in notes.iter_mut() {
                if note.channel >= AREA_LOCK_BASE {
                    continue;
                }
                let (index, count) = match params.rows {
                    Some(_) => {
                        let list = rows.get(&note.key).map(Vec::as_slice).unwrap_or(&[]);
                        (ordinal(list, note.tick), list.len() as i64)
                    }
                    None => {
                        let list = rows.get(&u16::MAX).map(Vec::as_slice).unwrap_or(&[]);
                        (ordinal(list, note.tick), list.len() as i64)
                    }
                };
                let index = if matches!(params.rows, Some(RowRestart::End)) {
                    count.saturating_sub(1).saturating_sub(index)
                } else {
                    index
                };
                set_channel(note, (index / every) + shift_of(note.key, lowest, shift), n);
            }
        }
        CycleBy::Key => {
            for note in notes.iter_mut() {
                if note.channel >= AREA_LOCK_BASE {
                    continue;
                }
                let row = i64::from(note.key.saturating_sub(lowest));
                set_channel(note, row / every + shift_of(note.key, lowest, shift), n);
            }
        }
        CycleBy::Time => {
            let Some(step) = time_step(params, ppq) else {
                return 0;
            };
            let bounds = row_bounds(notes);
            for note in notes.iter_mut() {
                if note.channel >= AREA_LOCK_BASE {
                    continue;
                }
                let index = match params.rows {
                    Some(RowRestart::First) => {
                        let start = bounds.get(&note.key).map(|(lo, _)| *lo).unwrap_or(0.0);
                        ((f64::from(note.tick) - f64::from(start)) / step)
                            .floor()
                            .max(0.0) as i64
                    }
                    Some(RowRestart::End) => {
                        let end = bounds.get(&note.key).map(|(_, hi)| *hi).unwrap_or(0.0);
                        ((f64::from(end) - f64::from(note.tick)) / step)
                            .floor()
                            .max(0.0) as i64
                    }
                    None => (f64::from(note.tick) / step).floor().max(0.0) as i64,
                };
                set_channel(note, index + shift_of(note.key, lowest, shift), n);
            }
        }
    }
    notes.len()
}

fn set_channel(note: &mut ShapeNote, index: i64, n: i64) {
    note.channel = index.rem_euclid(n) as u8;
}

fn shift_of(key: u16, lowest: u16, shift: Option<(i64, i64)>) -> i64 {
    let Some((s, m)) = shift else {
        return 0;
    };
    (i64::from(key.saturating_sub(lowest)) / m) * s
}

fn time_step(params: &CycleParams, ppq: f64) -> Option<f64> {
    let [a, b] = params.every_time.unwrap_or([1, 4]);
    if a == 0 || b == 0 || !ppq.is_finite() || ppq <= 0.0 {
        return None;
    }
    let step = 4.0 * ppq * f64::from(a) / f64::from(b);
    (step.is_finite() && step > 0.0).then_some(step)
}

/// Step 模式：`rows=Some` 每 key 一张起点表；`None` 用 `u16::MAX` 存全局表
fn step_rows(notes: &[ShapeNote], rows: Option<RowRestart>) -> BTreeMap<u16, Vec<f32>> {
    let mut map: BTreeMap<u16, Vec<f32>> = BTreeMap::new();
    for note in notes.iter().filter(|note| note.channel < AREA_LOCK_BASE) {
        let key = if rows.is_some() { note.key } else { u16::MAX };
        map.entry(key).or_default().push(note.tick);
    }
    for ticks in map.values_mut() {
        ticks.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        ticks.dedup_by(|a, b| (*a - *b).abs() <= EPS);
    }
    map
}

fn row_bounds(notes: &[ShapeNote]) -> BTreeMap<u16, (f32, f32)> {
    let mut map: BTreeMap<u16, (f32, f32)> = BTreeMap::new();
    for note in notes.iter().filter(|note| note.channel < AREA_LOCK_BASE) {
        let entry = map.entry(note.key).or_insert((note.tick, note.tick));
        entry.0 = entry.0.min(note.tick);
        entry.1 = entry.1.max(note.tick);
    }
    map
}

fn ordinal(ticks: &[f32], tick: f32) -> i64 {
    ticks
        .iter()
        .position(|value| (*value - tick).abs() <= EPS)
        .unwrap_or(0) as i64
}

#[cfg(test)]
mod tests;
