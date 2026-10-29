//! 形状音符生成：ShapeStyle.fill 的描边 / 内部填充分支
//!
//! 从 `shape_ops.rs` 拆出（文件长度纪律）：`shape_notes_of` 只做几何/算子编排，
//! 这里集中处理 Empty / OutlineSpam / Fill / Spam 四态，避免生成入口继续膨胀。

use crate::Editor;
use crate::interaction::line_tool::fill::spam_layout::{Span, place_gates};
use crate::interaction::line_tool::fill::spans::row_spans;
use crate::interaction::line_tool::{paths, stroke_band};
use lumino_core::shape::{
    AREA_LOCK_BASE, AREA_UNASSIGNED, COLOURS, GateRangeParams, RangeDir, ShapeModel, ShapeOp,
    ShapeRect,
};
use lumino_editor_state::ShapeNote;
use lumino_editor_state::shape_ops::gate_range::{
    fit_squares, keys_gate, normalize, range_squares,
};
use lumino_editor_state::shape_ops::{AreaContext, RowSpan, row_pieces_coloured};

impl Editor {
    /// 描边音符；`spam` 时按 Spam 门长/Align/Ends 切开（OutlineSpam）
    pub(super) fn shape_outline_notes(
        &self,
        poly: &[(f64, f64)],
        track: usize,
        key_count: i32,
        min_ticks: f32,
        spam: bool,
        model: &ShapeModel,
    ) -> Vec<ShapeNote> {
        let raw = stroke_band::outline_notes(
            poly,
            self.draw_thickness(),
            self.editor_state.view.zoom_x,
            self.editor_state.view.zoom_y,
            key_count,
        );
        let velocity = model.style.velocity.max(1);
        let gate = self.spam_gate_ticks(model);
        let mut out = Vec::new();
        for note in paths::keep_longest(&raw) {
            if note.key < 0 || note.key >= key_count {
                continue;
            }
            let (tick, key, length) = paths::to_note_triple(&note, min_ticks);
            if spam {
                for span in place_gates(
                    &[Span {
                        start: tick,
                        end: tick + length,
                    }],
                    gate,
                    model.style.align,
                    model.style.ends,
                    true,
                ) {
                    out.push(ShapeNote {
                        track,
                        tick: span.start,
                        key,
                        length: span.end - span.start,
                        velocity,
                        channel: 0,
                    });
                }
            } else {
                out.push(ShapeNote {
                    track,
                    tick,
                    key,
                    length,
                    velocity,
                    channel: 0,
                });
            }
        }
        out
    }

    /// 内部填充；`spam` 时按门长铺门（Fill / Spam）
    pub(super) fn shape_fill_notes(
        &self,
        poly: &[(f64, f64)],
        track: usize,
        key_count: i32,
        spam: bool,
        model: &ShapeModel,
        area: Option<&AreaContext>,
    ) -> Vec<ShapeNote> {
        let min_ticks = paths::min_note_length_ticks(
            self.editor_state.line_tool.min_note_division,
            self.editor_state.view.ppq,
        );
        if !model.geometry.closed || poly.len() < 3 {
            // 开放形状没有内部：退回描边，避免音符整块消失
            return self.shape_outline_notes(poly, track, key_count, min_ticks, false, model);
        }
        let mut poly32: Vec<(f32, f32)> = poly
            .iter()
            .map(|&(tick, key)| (tick as f32, key as f32))
            .collect();
        if poly32.first() != poly32.last()
            && let Some(&first) = poly32.first()
        {
            poly32.push(first);
        }
        let velocity = model.style.velocity.max(1);
        let gate = self.spam_gate_ticks(model);
        let range = model.ops.iter().find_map(|op| match op {
            ShapeOp::GateRange(params) => Some(params),
            _ => None,
        });
        let range_data = range.map(|params| {
            let (dir, graph) = normalize(params);
            (params, dir, graph)
        });
        let gate0 = gate.round().max(1.0) as i64;
        let gate1 = range
            .map(|params| {
                (params.to * f32::from(self.editor_state.view.ppq))
                    .round()
                    .max(1.0) as i64
            })
            .unwrap_or(gate0);
        let k0 = poly32
            .iter()
            .map(|(_, key)| *key)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as u16;
        let k1 = poly32
            .iter()
            .map(|(_, key)| *key)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .max(1.0) as u16;
        let k1 = k1.max(k0.saturating_add(1));
        let row_spans_all: Vec<(u16, Vec<Span>)> = (0..key_count)
            .map(|key| {
                let spans = row_spans(&poly32, key as f32)
                    .into_iter()
                    .map(|(start, end)| Span { start, end })
                    .collect();
                (key as u16, spans)
            })
            .collect();
        let (time_lo, time_hi) = row_spans_all
            .iter()
            .flat_map(|(_, spans)| spans.iter())
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), span| {
                (lo.min(span.start), hi.max(span.end))
            });
        let global_squares = range_data.as_ref().and_then(|(params, dir, graph)| {
            if spam
                && *dir == RangeDir::Time
                && !params.rows
                && time_lo.is_finite()
                && time_hi > time_lo
            {
                Some(range_squares(
                    time_lo.round() as i64,
                    time_hi.round() as i64,
                    gate0,
                    gate1,
                    graph,
                    params.halves,
                ))
            } else {
                None
            }
        });
        let mut out = Vec::new();
        for (key, spans) in row_spans_all {
            if spans.is_empty() {
                continue;
            }
            let pieces = if spam {
                match (&range_data, &global_squares) {
                    (Some((params, dir, graph)), Some(squares)) if *dir == RangeDir::Time => {
                        time_range_spans(&spans, squares, params)
                    }
                    (Some((params, dir, graph)), _) if *dir == RangeDir::Time => {
                        time_row_spans(&spans, params, gate0, gate1, graph)
                    }
                    (Some((params, _, graph)), _) => {
                        keys_range_spans(&spans, params, (gate0, gate1), graph, key, k0, k1)
                    }
                    _ => place_gates(&spans, gate, model.style.align, model.style.ends, true),
                }
            } else {
                spans
            };
            // 区域色只影响内部填充：未着色段保持 channel 0，之后仍可被 Cycle 接管；
            // 显式区域色写成 AREA_LOCK_BASE + colour，Cycle 会跳过，生成结束后统一解锁。
            let coloured: Vec<(RowSpan, u8)> = match area {
                Some(ctx) => {
                    let rows: Vec<RowSpan> = pieces
                        .iter()
                        .map(|span| RowSpan {
                            key,
                            start: span.start,
                            end: span.end,
                        })
                        .collect();
                    row_pieces_coloured(&rows, &ctx.map, ctx.box_)
                }
                None => pieces
                    .iter()
                    .map(|span| {
                        (
                            RowSpan {
                                key,
                                start: span.start,
                                end: span.end,
                            },
                            AREA_UNASSIGNED,
                        )
                    })
                    .collect(),
            };
            for (span, colour) in coloured {
                let Some(channel) = area_channel(colour) else {
                    continue;
                };
                if span.end > span.start {
                    out.push(ShapeNote {
                        track,
                        tick: span.start,
                        key,
                        length: span.end - span.start,
                        velocity,
                        channel,
                    });
                }
            }
        }
        if out.is_empty() {
            // 显式 Areas 可能把整个内部标空；此时不能再退回描边，否则“空区域”失效。
            if area.is_some() {
                return Vec::new();
            }
            // 自交/退化轮廓可能一行区间都没有；保持可见性
            return self.shape_outline_notes(poly, track, key_count, min_ticks, false, model);
        }
        out
    }

    /// Spam 门长（tick）：`gate_beats × ppq`，至少 1 tick
    fn spam_gate_ticks(&self, model: &ShapeModel) -> f32 {
        (model.style.gate_beats * f32::from(self.editor_state.view.ppq))
            .round()
            .max(1.0)
    }

    /// 由已展平路径构建区域上下文；`areas` 为空或没有有效着色点时返回 `None`
    pub(super) fn area_context(
        &self,
        paths: &[Vec<(f64, f64)>],
        model: &ShapeModel,
    ) -> Option<AreaContext> {
        let box_ = area_box_of(paths)?;
        // 当前工程配置尚无“启用通道 10”开关；AreaSpot 的 16 先按不可用处理。
        AreaContext::build(paths, box_, &model.style.areas, model.style.union, false)
    }
}

/// 形状自身 box（不做 Glue 的 key ±0.5 扩张；区域坐标以真实几何为准）
fn area_box_of(paths: &[Vec<(f64, f64)>]) -> Option<ShapeRect> {
    let mut points = paths.iter().flatten();
    let &(first_tick, first_key) = points.next()?;
    let mut tick_lo = first_tick;
    let mut tick_hi = first_tick;
    let mut key_lo = first_key;
    let mut key_hi = first_key;
    for &(tick, key) in points {
        tick_lo = tick_lo.min(tick);
        tick_hi = tick_hi.max(tick);
        key_lo = key_lo.min(key);
        key_hi = key_hi.max(key);
    }
    Some(ShapeRect::normalized(
        tick_lo as f32,
        key_lo as f32,
        tick_hi as f32,
        key_hi as f32,
    ))
}

/// 区域色 → 生成期 channel；`None` = 该段不产音符
fn area_channel(colour: u8) -> Option<u8> {
    match colour {
        AREA_UNASSIGNED => Some(0),
        0 => None,
        value if value <= COLOURS => Some(AREA_LOCK_BASE + value),
        // 16 需要工程启用通道 10；当前无该配置，生成阶段直接跳过。
        _ => None,
    }
}

/// 生成结束后把区域锁定 channel 还原成合法 MIDI 通道
pub(super) fn unlock_area_channels(notes: &mut [ShapeNote]) {
    for note in notes {
        if note.channel >= AREA_LOCK_BASE {
            note.channel -= AREA_LOCK_BASE;
        }
    }
}

fn time_range_spans(spans: &[Span], squares: &[(i64, i64)], params: &GateRangeParams) -> Vec<Span> {
    let mut out = Vec::new();
    for span in spans {
        let mut pieces = clip_squares(span, squares);
        if params.fit {
            fit_spans(&mut pieces, *span);
        }
        out.extend(pieces);
    }
    out
}

fn time_row_spans(
    spans: &[Span],
    params: &GateRangeParams,
    gate0: i64,
    gate1: i64,
    graph: &[[f32; 2]],
) -> Vec<Span> {
    let groups: Vec<Span> = if params.join && spans.len() > 1 {
        let start = spans.iter().map(|s| s.start).fold(f32::INFINITY, f32::min);
        let end = spans
            .iter()
            .map(|s| s.end)
            .fold(f32::NEG_INFINITY, f32::max);
        vec![Span { start, end }]
    } else {
        spans.to_vec()
    };
    let mut out = Vec::new();
    for group in groups {
        let mut squares = range_squares(
            group.start.round() as i64,
            group.end.round() as i64,
            gate0,
            gate1,
            graph,
            params.halves,
        );
        if params.fit {
            fit_squares(
                &mut squares,
                group.start.round() as i64,
                group.end.round() as i64,
            );
        }
        out.extend(clip_squares(&group, &squares));
    }
    out
}

fn keys_range_spans(
    spans: &[Span],
    params: &GateRangeParams,
    gates: (i64, i64),
    graph: &[[f32; 2]],
    key: u16,
    k0: u16,
    k1: u16,
) -> Vec<Span> {
    let (gate0, gate1) = gates;
    let gate = keys_gate(key, k0, k1, gate0, gate1, graph, params.halves).max(1);
    let mut out = Vec::new();
    for span in spans {
        let start = span.start.round() as i64;
        let end = span.end.round() as i64;
        let mut local = Vec::new();
        let mut cursor = start;
        while cursor < end {
            local.push(Span {
                start: cursor as f32,
                end: (cursor + gate).min(end) as f32,
            });
            cursor += gate;
        }
        if params.fit {
            fit_spans(&mut local, *span);
        }
        out.extend(local);
    }
    out
}

fn clip_squares(span: &Span, squares: &[(i64, i64)]) -> Vec<Span> {
    squares
        .iter()
        .filter_map(|(start, end)| {
            let a = span.start.max(*start as f32);
            let b = span.end.min(*end as f32);
            (b > a).then_some(Span { start: a, end: b })
        })
        .collect()
}

fn fit_spans(pieces: &mut [Span], span: Span) {
    if pieces.is_empty() {
        return;
    }
    pieces[0].start = span.start;
    let last = pieces.len() - 1;
    pieces[last].end = span.end;
}

#[cfg(test)]
mod tests;
