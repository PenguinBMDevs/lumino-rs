//! 形状音符生成：ShapeStyle.fill 的描边 / 内部填充分支
//!
//! 从 `shape_ops.rs` 拆出（文件长度纪律）：`shape_notes_of` 只做几何/算子编排，
//! 这里集中处理 Empty / OutlineSpam / Fill / Spam 四态，避免生成入口继续膨胀。

use crate::Editor;
use crate::interaction::line_tool::fill::spam_layout::{Span, place_gates};
use crate::interaction::line_tool::fill::spans::row_spans;
use crate::interaction::line_tool::{paths, stroke_band};
use lumino_core::shape::{GateRangeParams, RangeDir, ShapeModel, ShapeOp};
use lumino_editor_state::ShapeNote;
use lumino_editor_state::shape_ops::gate_range::{
    fit_squares, keys_gate, normalize, range_squares,
};

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
            for span in pieces {
                if span.end > span.start {
                    out.push(ShapeNote {
                        track,
                        tick: span.start,
                        key,
                        length: span.end - span.start,
                        velocity,
                        channel: 0,
                    });
                }
            }
        }
        if out.is_empty() {
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
