//! Tiles 内部填充（DRAW-017）
//!
//! 单块 span 预计算一次，再按网格偏移复制；`Brick` 奇数行错半块，
//! `flip` 按行列镜像，`by` 决定块通道，`whole` 丢弃被轮廓切到的块。

use lumino_core::shape::{TileBy, TileFlip, TileLayout, TilesParams};

use super::masks::RowSpan;
use super::nested::ColouredSpan;

/// 生成平铺片段
pub fn tiled_spans(area: &[RowSpan], tile: &[RowSpan], params: &TilesParams) -> Vec<ColouredSpan> {
    if area.is_empty() || tile.is_empty() {
        return Vec::new();
    }
    let Some((min_tick, max_tick, min_key, max_key)) = bounds(area) else {
        return Vec::new();
    };
    let cell_w = params.length.max(1.0);
    let cell_h = params.keys.max(1.0);
    let stride_x = cell_w + params.gap.max(0.0);
    let stride_y = cell_h + params.gap_keys.max(0.0);
    if stride_x <= 0.0 || stride_y <= 0.0 {
        return Vec::new();
    }
    let origin = params.at.map_or((min_tick, min_key), |at| {
        (
            min_tick + at[0] * (max_tick - min_tick),
            min_key + at[1] * (max_key - min_key),
        )
    });
    let mut out = Vec::new();
    let mut row = 0usize;
    loop {
        let cell_y = origin.1 + row as f32 * stride_y;
        if cell_y > max_key {
            break;
        }
        let row_offset = if params.layout == TileLayout::Brick && row % 2 == 1 {
            stride_x * 0.5
        } else {
            0.0
        };
        let mut col = 0usize;
        loop {
            let cell_x = origin.0 + col as f32 * stride_x + row_offset;
            if cell_x > max_tick {
                break;
            }
            for span in tile {
                if span.end <= span.start {
                    continue;
                }
                let local_start = span.start;
                let local_end = span.end;
                if local_start < 0.0 || local_end > cell_w {
                    continue;
                }
                let (mut x0, mut x1) = (cell_x + local_start, cell_x + local_end);
                let mirror_h = matches!(params.flip, TileFlip::H | TileFlip::Hv) && col % 2 == 1;
                if mirror_h {
                    let a = cell_x + (cell_w - local_end);
                    let b = cell_x + (cell_w - local_start);
                    x0 = a;
                    x1 = b;
                }
                let mut key = cell_y + span.key as f32;
                let mirror_v = matches!(params.flip, TileFlip::V | TileFlip::Hv) && row % 2 == 1;
                if mirror_v {
                    key = cell_y + (cell_h - span.key as f32 - 1.0);
                }
                let key = key.round();
                if !(0.0..=255.0).contains(&key) {
                    continue;
                }
                let key = key as u16;
                if params.whole && !fully_inside(area, key, x0, x1) {
                    continue;
                }
                let colour = colour_for(params, row, col);
                for piece in clip_to_area(area, key, x0, x1) {
                    out.push(ColouredSpan {
                        key,
                        start: piece.0,
                        end: piece.1,
                        colour,
                    });
                }
            }
            col += 1;
            if cell_x + cell_w > max_tick + stride_x {
                break;
            }
        }
        row += 1;
        if cell_y + cell_h > max_key + stride_y {
            break;
        }
    }
    out.sort_by(|a, b| {
        a.key
            .cmp(&b.key)
            .then(
                a.start
                    .partial_cmp(&b.start)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
            .then(
                a.end
                    .partial_cmp(&b.end)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    out.dedup();
    out
}

fn colour_for(params: &TilesParams, row: usize, col: usize) -> u8 {
    if params.colours == 0 || params.by == TileBy::One {
        return 0;
    }
    let n = usize::from(params.colours.min(16));
    let index = match params.by {
        TileBy::Diag => row + col,
        TileBy::Cols => col,
        TileBy::Rows => row,
        TileBy::Checker => (row + col) % 2,
        TileBy::One => 0,
    };
    (index % n) as u8
}

fn fully_inside(area: &[RowSpan], key: u16, start: f32, end: f32) -> bool {
    area.iter()
        .filter(|span| span.key == key)
        .any(|span| span.start <= start + 1e-4 && span.end >= end - 1e-4)
}

fn clip_to_area(area: &[RowSpan], key: u16, start: f32, end: f32) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for span in area.iter().filter(|span| span.key == key) {
        let a = start.max(span.start);
        let b = end.min(span.end);
        if b > a {
            out.push((a, b));
        }
    }
    out
}

fn bounds(area: &[RowSpan]) -> Option<(f32, f32, f32, f32)> {
    let mut min_tick = f32::INFINITY;
    let mut max_tick = f32::NEG_INFINITY;
    let mut min_key = f32::INFINITY;
    let mut max_key = f32::NEG_INFINITY;
    for span in area {
        if !span.start.is_finite() || !span.end.is_finite() || span.end <= span.start {
            continue;
        }
        min_tick = min_tick.min(span.start);
        max_tick = max_tick.max(span.end);
        min_key = min_key.min(f32::from(span.key));
        max_key = max_key.max(f32::from(span.key));
    }
    (min_tick.is_finite() && max_tick > min_tick && max_key >= min_key)
        .then_some((min_tick, max_tick, min_key, max_key))
}

#[cfg(test)]
mod tests;
