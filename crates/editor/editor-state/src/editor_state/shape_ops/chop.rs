//! Chop（节奏切分）音符算法
//!
//! 按节奏型切任意音符：节奏沿每个音符重复，`abs` 时从 tick 0 对齐；
//! 同 key 相接的音符先合并成 run 再切；超限整体放弃（不产出半套结果）。

use lumino_core::shape::ChopParams;

use crate::ShapeNote;

const EPS: f64 = 1e-6;
const MAX_PIECES: usize = 20_000_000;

/// Chop 执行报告
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ChopReport {
    /// 实际产出的片数
    pub pieces: usize,
    /// 是否因超过 2e7 片而放弃（放弃时返回原 notes）
    pub gave_up: bool,
}

/// 应用 Chop；`ppq` 用于拍 → tick
pub fn apply(notes: &[ShapeNote], params: &ChopParams, ppq: f64) -> (Vec<ShapeNote>, ChopReport) {
    apply_with_limit(notes, params, ppq, MAX_PIECES)
}

fn apply_with_limit(
    notes: &[ShapeNote],
    params: &ChopParams,
    ppq: f64,
    limit: usize,
) -> (Vec<ShapeNote>, ChopReport) {
    let empty = ChopReport::default();
    if notes.is_empty() || !ppq.is_finite() || ppq <= 0.0 {
        return (notes.to_vec(), empty);
    }
    let step_ticks = if params.len > 0.0 {
        params.len as f64 * ppq
    } else {
        1.0
    };
    let cycle_ticks = params.steps as f64 * step_ticks;
    if !step_ticks.is_finite() || step_ticks <= 0.0 || !cycle_ticks.is_finite() {
        return (notes.to_vec(), empty);
    }
    if params.pieces.is_empty() {
        return (Vec::new(), empty);
    }

    let runs = build_runs(notes);
    let mut out = Vec::new();
    let mut total = 0usize;
    for run in &runs {
        let origin = if params.abs {
            (run.start / cycle_ticks).floor() * cycle_ticks
        } else {
            run.start
        };
        let mut cycle_start = origin;
        while cycle_start < run.end - EPS {
            for piece in &params.pieces {
                let piece_start = cycle_start + piece.start as f64 * step_ticks;
                if piece_start >= run.end - EPS {
                    continue;
                }
                let piece_len = if piece.len > 0.0 {
                    piece.len as f64 * step_ticks
                } else {
                    1.0
                };
                let start = piece_start.max(run.start);
                let end = (piece_start + piece_len).min(run.end);
                if end <= start + EPS {
                    continue;
                }
                total += 1;
                if total > limit {
                    return (
                        notes.to_vec(),
                        ChopReport {
                            pieces: total,
                            gave_up: true,
                        },
                    );
                }
                out.push(ShapeNote {
                    track: run.track,
                    tick: start as f32,
                    key: run.key,
                    length: (end - start) as f32,
                    velocity: piece_velocity(run.velocity, piece.vel, params.vel, params.fixed),
                });
            }
            cycle_start += cycle_ticks;
            if !cycle_start.is_finite() {
                break;
            }
        }
    }
    (
        out,
        ChopReport {
            pieces: total,
            gave_up: false,
        },
    )
}

struct Run {
    track: usize,
    key: u16,
    start: f64,
    end: f64,
    velocity: u8,
}

/// 同一 (track, key) 相接/重叠的音符合并为一条 run
fn build_runs(notes: &[ShapeNote]) -> Vec<Run> {
    let mut indexed: Vec<(usize, &ShapeNote)> = notes.iter().enumerate().collect();
    indexed.sort_by(|(ia, a), (ib, b)| {
        a.track
            .cmp(&b.track)
            .then(a.key.cmp(&b.key))
            .then(
                a.tick
                    .partial_cmp(&b.tick)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(ia.cmp(ib))
    });
    let mut runs: Vec<Run> = Vec::new();
    for (_, note) in indexed {
        let start = note.tick as f64;
        let end = start + note.length.max(0.0) as f64;
        if let Some(last) = runs.last_mut()
            && last.track == note.track
            && last.key == note.key
            && start <= last.end + EPS
        {
            last.end = last.end.max(end);
            continue;
        }
        runs.push(Run {
            track: note.track,
            key: note.key,
            start,
            end,
            velocity: note.velocity,
        });
    }
    runs
}

fn piece_velocity(note: u8, piece: u8, weight: u8, fixed: bool) -> u8 {
    let mix = weight as f32 / 100.0;
    let value = if fixed {
        (1.0 - mix) * note as f32 + mix * piece as f32
    } else {
        note as f32 * (1.0 - mix + mix * piece as f32 / 100.0)
    };
    value.round().clamp(1.0, 127.0) as u8
}

#[cfg(test)]
mod tests;
