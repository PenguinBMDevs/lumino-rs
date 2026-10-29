//! 形状区域着色（DRAW-032）：AreaMap 构建与逐行切分
//!
//! 核心网格在 `lumino_core::shape::areas`；这里负责 tick/key → 归一化坐标、
//! 把 `ShapeStyle.areas` 落到网格，以及把生成侧的 `RowSpan` 按区域色切开。
//! 区域只影响**内部填充**；描边/门长/其它 Notes 算子仍走原链路。

use lumino_core::shape::{ALL_COLOURS, AreaMap, AreaSpot, MAP_CELLS, ShapeRect, most_colours};

use super::masks::RowSpan;

/// 一次生成使用的区域上下文
#[derive(Debug, Clone, PartialEq)]
pub struct AreaContext {
    /// 已求解的区域网格
    pub map: AreaMap,
    /// 网格对应的形状 box（tick/key 逻辑坐标）
    pub box_: ShapeRect,
}

impl AreaContext {
    /// 构造上下文；`spots` 为空或没有任何点落在形状内时返回 `None`
    pub fn build(
        paths: &[Vec<(f64, f64)>],
        box_: ShapeRect,
        spots: &[AreaSpot],
        union: bool,
        use10: bool,
    ) -> Option<Self> {
        if spots.is_empty() || paths.is_empty() {
            return None;
        }
        let width = box_.tick_hi - box_.tick_lo;
        let height = box_.key_hi - box_.key_lo;
        if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
            return None;
        }

        let mut normalized = Vec::with_capacity(paths.len());
        for path in paths {
            if path.len() < 2 {
                continue;
            }
            let points: Vec<(f32, f32)> = path
                .iter()
                .map(|&(tick, key)| {
                    (
                        ((tick as f32 - box_.tick_lo) / width).clamp(0.0, 1.0),
                        ((key as f32 - box_.key_lo) / height).clamp(0.0, 1.0),
                    )
                })
                .collect();
            normalized.push(points);
        }
        if normalized.is_empty() {
            return None;
        }

        let mut map = AreaMap::rasterize(&normalized, union);
        let max_colour = most_colours(use10);
        let mut painted = false;
        for spot in spots {
            if spot.colour > max_colour {
                continue;
            }
            painted |= map.paint_area(spot.u, spot.v, spot.colour);
        }
        painted.then_some(Self { map, box_ })
    }
}

/// 把一行（或一段）区间按区域网格切分
///
/// 返回的每一段都带网格色号；`AREA_UNASSIGNED` 表示该段没有显式区域色，
/// 调用方应沿用原来的填充规则。相邻且同色的段会合并，避免 1024 网格把
/// 一条长音切成上千段。边界格会向两格内的邻居借色，最多允许一格误差
/// （对齐上游“细碎片取邻居色”的取舍）。
pub fn row_pieces_coloured(rows: &[RowSpan], map: &AreaMap, box_: ShapeRect) -> Vec<(RowSpan, u8)> {
    let width = box_.tick_hi - box_.tick_lo;
    let height = box_.key_hi - box_.key_lo;
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return rows
            .iter()
            .copied()
            .map(|row| (row, lumino_core::shape::AREA_UNASSIGNED))
            .collect();
    }

    let cells = f32::from(MAP_CELLS);
    let mut out: Vec<(RowSpan, u8)> = Vec::new();
    for row in rows {
        if row.end <= row.start {
            continue;
        }
        let v_raw = (row.key as f32 + 0.5 - box_.key_lo) / height;
        // 填充按整数 key 行生成，边界行可能落在 box 外半行；夹到边界后向邻格借色。
        if !v_raw.is_finite() || !(-0.5..=1.5).contains(&v_raw) {
            push_merged(
                &mut out,
                RowSpan {
                    key: row.key,
                    start: row.start,
                    end: row.end,
                },
                lumino_core::shape::AREA_UNASSIGNED,
            );
            continue;
        }
        let v = v_raw.clamp(0.0, 1.0);

        let u0 = ((row.start - box_.tick_lo) / width).clamp(0.0, 1.0);
        let u1 = ((row.end - box_.tick_lo) / width).clamp(0.0, 1.0);
        if u1 <= u0 {
            push_merged(
                &mut out,
                RowSpan {
                    key: row.key,
                    start: row.start,
                    end: row.end,
                },
                lumino_core::shape::AREA_UNASSIGNED,
            );
            continue;
        }

        let first = ((u0 * cells).floor() as usize).min(usize::from(MAP_CELLS) - 1);
        let last = ((u1 * cells).ceil() as usize).min(usize::from(MAP_CELLS));
        for col in first..last {
            let tick_start = box_.tick_lo + (col as f32 / cells) * width;
            let tick_end = box_.tick_lo + ((col + 1) as f32 / cells) * width;
            let start = tick_start.max(row.start);
            let end = tick_end.min(row.end);
            if end <= start {
                continue;
            }
            let colour = sample_colour(map, (col as f32 + 0.5) / cells, v);
            push_merged(
                &mut out,
                RowSpan {
                    key: row.key,
                    start,
                    end,
                },
                colour,
            );
        }
    }
    out
}

/// 取网格色；边界格向四邻借色（与“细碎片取邻居色”的上游取舍一致）
fn sample_colour(map: &AreaMap, u: f32, v: f32) -> u8 {
    let colour = map.area_at(u, v);
    if colour <= ALL_COLOURS {
        return colour;
    }
    // 跨两格：边界线本身占一格，单格偏移可能仍落在边界行/列。
    let step = 2.0 / f32::from(MAP_CELLS);
    for (du, dv) in [
        (step, 0.0),
        (-step, 0.0),
        (0.0, step),
        (0.0, -step),
        (step, step),
        (step, -step),
        (-step, step),
        (-step, -step),
    ] {
        let neighbour = map.area_at((u + du).clamp(0.0, 1.0), (v + dv).clamp(0.0, 1.0));
        if neighbour <= ALL_COLOURS {
            return neighbour;
        }
    }
    lumino_core::shape::AREA_UNASSIGNED
}

fn push_merged(out: &mut Vec<(RowSpan, u8)>, span: RowSpan, colour: u8) {
    if let Some((last, last_colour)) = out.last_mut()
        && last.key == span.key
        && *last_colour == colour
        && (last.end - span.start).abs() <= 1e-4
    {
        last.end = span.end;
        return;
    }
    out.push((span, colour));
}

#[cfg(test)]
mod tests;
