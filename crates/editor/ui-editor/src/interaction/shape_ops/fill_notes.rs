//! 形状音符生成：ShapeStyle.fill 的描边 / 内部填充分支
//!
//! 从 `shape_ops.rs` 拆出（文件长度纪律）：`shape_notes_of` 只做几何/算子编排，
//! 这里集中处理 Empty / OutlineSpam / Fill / Spam 四态，避免生成入口继续膨胀。

use crate::Editor;
use crate::interaction::line_tool::fill::spam_layout::{Span, place_gates};
use crate::interaction::line_tool::fill::spans::row_spans;
use crate::interaction::line_tool::{paths, stroke_band};
use lumino_core::shape::ShapeModel;
use lumino_editor_state::ShapeNote;

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
        let mut out = Vec::new();
        for key in 0..key_count {
            let spans: Vec<Span> = row_spans(&poly32, key as f32)
                .into_iter()
                .map(|(start, end)| Span { start, end })
                .collect();
            if spans.is_empty() {
                continue;
            }
            let pieces = if spam {
                place_gates(&spans, gate, model.style.align, model.style.ends, true)
            } else {
                spans
            };
            for span in pieces {
                if span.end > span.start {
                    out.push(ShapeNote {
                        track,
                        tick: span.start,
                        key: key as u16,
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
