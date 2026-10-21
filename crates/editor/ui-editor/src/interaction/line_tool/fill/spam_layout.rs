//! Spam 门长落位：Align 三态 × Ends 五态
//!
//! 移植自 Spiderweb `scripts/notes/custom.py` 的 `chop`（2110-2406 行）。
//! 所有计算先 round 到整 tick，避免 f32 累积误差；输出仍是 f32 tick 供现有链路使用。
//!
//! 这是门长落位的**唯一实现**：曲线/形状填充与 DRAW-013 Gate Range 都复用它，
//! 禁止在 confirm/fill/region 各写一套。

use lumino_core::shape::{SpamAlign, SpamEnds};

/// 一段连续填充区间（同一 key 行）
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Span {
    /// 起点（tick）
    pub start: f32,
    /// 终点（tick）
    pub end: f32,
}

/// 按 Align/Ends 把连续区间铺成门
///
/// `tick0_aligned`：`Align::Aligned` 的网格原点是否为绝对 tick 0。
/// 为 `false` 时以所有 span 的最左起点为原点（相对块），便于宿主把一段相对几何
/// 原样搬到绝对时间轴。
pub(crate) fn place_gates(
    spans: &[Span],
    gate: f32,
    align: SpamAlign,
    ends: SpamEnds,
    tick0_aligned: bool,
) -> Vec<Span> {
    if spans.is_empty() || !gate.is_finite() || gate < 1.0 {
        return spans.to_vec();
    }
    let g = gate.round().max(1.0) as i64;
    let origin = if tick0_aligned {
        0
    } else {
        spans
            .iter()
            .map(|span| span.start.round() as i64)
            .min()
            .unwrap_or(0)
    };
    let mut out = Vec::new();
    for span in spans {
        let start = span.start.round() as i64 - origin;
        let end = span.end.round() as i64 - origin;
        if end <= start {
            continue;
        }
        let size = end - start;
        match ends {
            SpamEnds::Stretch => {
                let n = ((2 * size + g) / (2 * g)).max(1);
                for k in 0..n {
                    let a = start + size * k / n;
                    let b = start + size * (k + 1) / n;
                    push(&mut out, a + origin, b + origin);
                }
            }
            SpamEnds::Round => {
                let n = ((2 * size + g) / (2 * g)).max(1);
                let (first, count) = match align {
                    SpamAlign::Aligned => {
                        let lo = -((g - 2 * start).div_euclid(2 * g));
                        let count = (2 * end + g).div_euclid(2 * g) - lo;
                        if count <= 0 {
                            // 整段不足半个门：取中点所在门，保证至少一条
                            ((start + end).div_euclid(2).div_euclid(g) * g, 1)
                        } else {
                            (lo * g, count)
                        }
                    }
                    SpamAlign::Centred => (start + (size - n * g).div_euclid(2), n),
                    SpamAlign::Auto => (start, n),
                };
                for k in 0..count {
                    let a = first + k * g;
                    push(&mut out, a + origin, a + g + origin);
                }
            }
            SpamEnds::Keep => {
                let s = match align {
                    SpamAlign::Aligned => ceil_to(start, g),
                    SpamAlign::Centred => {
                        if size < g {
                            start
                        } else {
                            start + size.rem_euclid(g) / 2
                        }
                    }
                    SpamAlign::Auto => start,
                };
                let n = if end > s { (end - s) / g } else { 0 };
                let head_end = s.min(end);
                let head = head_end > start;
                let tail = if s < end { s + n * g } else { end } < end;
                let count = n + head as i64 + tail as i64;
                for k in 0..count {
                    let mut a = s + (k - head as i64) * g;
                    let mut b = a + g;
                    if head && k == 0 {
                        a = start;
                        b = head_end;
                    }
                    if tail && k == count - 1 {
                        b = end;
                    }
                    push(&mut out, a + origin, b + origin);
                }
            }
            SpamEnds::Drop | SpamEnds::Min => {
                let s = match align {
                    SpamAlign::Aligned => ceil_to(start, g),
                    SpamAlign::Centred => {
                        if size < g {
                            start
                        } else {
                            start + size.rem_euclid(g) / 2
                        }
                    }
                    SpamAlign::Auto => start,
                };
                let n = if end > s { (end - s) / g } else { 0 };
                if n == 0 {
                    let (mut a, mut b) = (start, end);
                    if ends == SpamEnds::Min {
                        let least = ((g + 3) / 4).max(1);
                        if b - a < least {
                            a += (b - a - least).div_euclid(2);
                            b = a + least;
                        }
                    }
                    push(&mut out, a + origin, b + origin);
                } else {
                    for k in 0..n {
                        let a = s + k * g;
                        push(&mut out, a + origin, a + g + origin);
                    }
                }
            }
        }
    }
    out
}

fn ceil_to(value: i64, step: i64) -> i64 {
    let rem = value.rem_euclid(step);
    if rem == 0 { value } else { value + step - rem }
}

fn push(out: &mut Vec<Span>, start: i64, end: i64) {
    if end > start {
        out.push(Span {
            start: start as f32,
            end: end as f32,
        });
    }
}

#[cfg(test)]
mod tests;
