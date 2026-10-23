//! Strum（扫弦）音符算法
//!
//! 同 tick 起音的和弦按指数级数依次错开起音/收尾/力度：
//! `steps(k) = g·(r^k − 1)/(r − 1)`，`r = 1 + tension/100`（≥0）或 `1 + tension/200`（<0）。

use lumino_core::shape::StrumParams;

use crate::ShapeNote;

const EPS: f64 = 1e-6;
const CAP_BEATS: f64 = 64.0;
const MAX_CHOP_PIECES: usize = 5_000_000;

/// 应用 Strum；`ppq` 用于拍 → tick
pub fn apply(notes: &[ShapeNote], params: &StrumParams, ppq: f64) -> Vec<ShapeNote> {
    if notes.is_empty() || !ppq.is_finite() || ppq <= 0.0 {
        return notes.to_vec();
    }
    let chopped;
    let notes = if params.chop {
        let Some(pieces) = chop_notes(notes) else {
            return notes.to_vec();
        };
        chopped = pieces;
        chopped.as_slice()
    } else {
        notes
    };

    let mut order: Vec<usize> = (0..notes.len()).collect();
    order.sort_by(|&a, &b| {
        notes[a]
            .tick
            .partial_cmp(&notes[b].tick)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(notes[a].key.cmp(&notes[b].key))
            .then(a.cmp(&b))
    });

    let mut out: Vec<Option<ShapeNote>> = notes.iter().copied().map(Some).collect();
    let mut chord_index = 0usize;
    let mut cursor = 0usize;
    while cursor < order.len() {
        let tick = notes[order[cursor]].tick;
        let mut end = cursor + 1;
        while end < order.len() && (notes[order[end]].tick - tick).abs() <= EPS as f32 {
            end += 1;
        }
        let mut group: Vec<usize> = order[cursor..end].to_vec();
        cursor = end;
        if group.len() < 2 {
            continue; // 单音和弦不参与移动（含 alternate）
        }

        let reverse = (params.time < 0.0) ^ (params.alternate && !chord_index.is_multiple_of(2));
        group.sort_by(|&a, &b| {
            let ordering = notes[a].key.cmp(&notes[b].key).then(a.cmp(&b));
            if reverse {
                ordering.reverse()
            } else {
                ordering
            }
        });
        chord_index += 1;

        let n = group.len();
        let cap = CAP_BEATS * ppq;
        let g = params.time.abs() as f64 * ppq;
        let offsets: Vec<f64> = (0..n)
            .map(|rank| steps(rank as i32, g, params.tension as f64, cap))
            .collect();
        let mut shift = 0.0;
        if params.ahead {
            let max = offsets.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            shift = -max;
            let min_start = group
                .iter()
                .zip(&offsets)
                .map(|(&index, &offset)| notes[index].tick as f64 + offset + shift)
                .fold(f64::INFINITY, f64::min);
            if min_start < 0.0 {
                shift += -min_start;
            }
        }

        for (rank, &index) in group.iter().enumerate() {
            let original = notes[index];
            let delta = offsets[rank] + shift;
            let original_end = original.tick as f64 + original.length.max(0.0) as f64;
            let mut start = original.tick as f64 + delta;
            let mut note_end = if params.preserve && !params.ahead {
                original_end
            } else {
                original_end + delta
            };
            if start < 0.0 {
                let correction = -start;
                start = 0.0;
                if !(params.preserve && !params.ahead) {
                    note_end += correction;
                }
            }

            if params.end && params.end_time != 0.0 {
                let end_rank = if params.end_time < 0.0 {
                    rank
                } else {
                    n - 1 - rank
                };
                let end_offset = steps(
                    end_rank as i32,
                    params.end_time.abs() as f64 * ppq,
                    params.end_tension as f64,
                    cap,
                );
                note_end = (note_end - end_offset).max(start + 1.0);
            }
            if note_end <= start + EPS {
                out[index] = None;
                continue;
            }

            let velocity_offset = steps(
                rank as i32,
                params.vel as f64,
                params.vel_tension as f64,
                127.0,
            );
            let velocity = (original.velocity as f64 + velocity_offset)
                .round()
                .clamp(1.0, 127.0) as u8;
            out[index] = Some(ShapeNote {
                track: original.track,
                tick: start as f32,
                key: original.key,
                length: (note_end - start) as f32,
                velocity,
                channel: original.channel,
            });
        }
    }

    out.into_iter().flatten().collect()
}

/// 指数级数第 `rank` 步的位移（已按 `cap` 封顶）
fn steps(rank: i32, g: f64, tension: f64, cap: f64) -> f64 {
    if rank <= 0 || g == 0.0 || !g.is_finite() {
        return 0.0;
    }
    let ratio = if tension >= 0.0 {
        1.0 + tension / 100.0
    } else {
        1.0 + tension / 200.0
    };
    let value = if (ratio - 1.0).abs() <= EPS {
        g * rank as f64
    } else {
        g * (ratio.powi(rank) - 1.0) / (ratio - 1.0)
    };
    value.clamp(-cap, cap)
}

/// 按「所有音符的起点」切分音符，使被长音压住的片段也能参与后续和弦
fn chop_notes(notes: &[ShapeNote]) -> Option<Vec<ShapeNote>> {
    let mut starts: Vec<f32> = notes.iter().map(|note| note.tick).collect();
    starts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    starts.dedup_by(|a, b| (*a - *b).abs() <= EPS as f32);

    let mut out = Vec::with_capacity(notes.len());
    for note in notes {
        let end = note.tick + note.length.max(0.0);
        let mut cursor = note.tick;
        for &split in &starts {
            if split <= cursor + EPS as f32 {
                continue;
            }
            if split >= end - EPS as f32 {
                break;
            }
            out.push(ShapeNote {
                tick: cursor,
                length: split - cursor,
                ..*note
            });
            cursor = split;
            if out.len() > MAX_CHOP_PIECES {
                return None;
            }
        }
        if end > cursor + EPS as f32 {
            out.push(ShapeNote {
                tick: cursor,
                length: end - cursor,
                ..*note
            });
        } else {
            out.push(*note);
        }
        if out.len() > MAX_CHOP_PIECES {
            return None;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests;
