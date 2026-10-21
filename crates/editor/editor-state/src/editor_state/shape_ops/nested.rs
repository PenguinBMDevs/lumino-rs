//! Nested 内部填充（DRAW-017）
//!
//! 把形状内部的逐行 span 按步进向 `at` 缩小，每层轮转一个通道。
//! `Scale` 用同一缩放因子；`Band` 用等厚内缩；`Blend` 在两者间混合。
//! `drawn_k` 是绘制时的 tick-per-key，缩放先在屏幕空间完成，保证圆不失真。

use lumino_core::shape::{NestedParams, NestedWay};

use super::masks::RowSpan;

/// 带通道的逐行片段
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColouredSpan {
    /// key 行
    pub key: u16,
    /// 起点（tick）
    pub start: f32,
    /// 终点（tick）
    pub end: f32,
    /// 通道 / 颜色（0..=15）
    pub colour: u8,
}

/// 生成 Nested 的各层片段
pub fn nested_spans(area: &[RowSpan], params: &NestedParams, drawn_k: f32) -> Vec<ColouredSpan> {
    if area.is_empty() || params.steps == 0 {
        return Vec::new();
    }
    let Some((min_tick, max_tick, min_key, max_key)) = bounds(area) else {
        return Vec::new();
    };
    let k = if drawn_k.is_finite() && drawn_k > 0.0 {
        drawn_k as f64
    } else {
        1.0
    };
    let at = params.at.map_or(
        (
            (f64::from(min_tick) + f64::from(max_tick)) / 2.0,
            (f64::from(min_key) + f64::from(max_key)) / 2.0,
        ),
        |at| {
            (
                f64::from(min_tick) + f64::from(at[0]) * f64::from(max_tick - min_tick),
                f64::from(min_key) + f64::from(at[1]) * f64::from(max_key - min_key),
            )
        },
    );
    let steps = usize::from(params.steps);
    let mut out = Vec::new();
    for index in 0..steps {
        if params.skip && index % 2 == 1 {
            continue;
        }
        let t = if steps <= 1 {
            0.0
        } else {
            index as f64 / (steps - 1) as f64
        };
        let base = 1.0 + (f64::from(params.last) - 1.0) * t;
        let base = (base + f64::from(params.spacing) / 100.0 * (1.0 - t) * 0.5).clamp(0.0, 1.0);
        let (fx, fy) = match params.way {
            NestedWay::Scale => (base, base),
            NestedWay::Blend => {
                let factor = 1.0 - f64::from(params.mix) * (1.0 - base);
                (factor, factor)
            }
            NestedWay::Band => {
                let width = f64::from(max_tick - min_tick).max(1.0);
                let height = f64::from(max_key - min_key).max(1.0);
                let inset = (1.0 - base) * 0.5 * width.min(height);
                (
                    (1.0 - 2.0 * inset / width).clamp(0.0, 1.0),
                    (1.0 - 2.0 * inset / height).clamp(0.0, 1.0),
                )
            }
        };
        let colour = colour_at(index, params.colours);
        for span in area {
            let x0 = f64::from(span.start) / k;
            let x1 = f64::from(span.end) / k;
            let y = f64::from(span.key);
            let nx0 = at.0 + (x0 - at.0) * fx;
            let nx1 = at.0 + (x1 - at.0) * fx;
            let ny = at.1 + (y - at.1) * fy;
            let key = ny.round();
            if !(0.0..=255.0).contains(&key) {
                continue;
            }
            let start = (nx0 * k) as f32;
            let end = (nx1 * k) as f32;
            if end > start {
                out.push(ColouredSpan {
                    key: key as u16,
                    start,
                    end,
                    colour,
                });
            }
        }
    }
    out.sort_by(|a, b| {
        a.key.cmp(&b.key).then(
            a.start
                .partial_cmp(&b.start)
                .unwrap_or(std::cmp::Ordering::Equal),
        )
    });
    out
}

fn colour_at(index: usize, colours: u8) -> u8 {
    if colours == 0 {
        0
    } else {
        (index % usize::from(colours.min(16))) as u8
    }
}

fn bounds(area: &[RowSpan]) -> Option<(f32, f32, f32, f32)> {
    let mut min_tick = f32::INFINITY;
    let mut max_tick = f32::NEG_INFINITY;
    let mut min_key = u16::MAX;
    let mut max_key = 0u16;
    for span in area {
        if !span.start.is_finite() || !span.end.is_finite() || span.end <= span.start {
            continue;
        }
        min_tick = min_tick.min(span.start);
        max_tick = max_tick.max(span.end);
        min_key = min_key.min(span.key);
        max_key = max_key.max(span.key);
    }
    (min_tick.is_finite() && max_tick > min_tick && min_key != u16::MAX).then_some((
        min_tick,
        max_tick,
        f32::from(min_key),
        f32::from(max_key),
    ))
}

#[cfg(test)]
mod tests;
