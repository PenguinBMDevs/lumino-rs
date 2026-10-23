//! Funnel 音符生成（DRAW-018 最简版）

use lumino_core::shape::{FunnelParams, ShapeFill};
use lumino_editor_state::ShapeNote;
use lumino_editor_state::shape_ops::{funnel_box_outline, funnel_rows, funnel_slant_boxes};

use crate::Editor;

impl Editor {
    /// 漏斗 → 音符：Empty 走 box 边界描边，Spam 按门长铺满斜 box
    pub(super) fn funnel_notes(
        &self,
        params: &FunnelParams,
        track: usize,
        model: &lumino_core::shape::ShapeModel,
        ppq: f64,
    ) -> Vec<ShapeNote> {
        let boxes = funnel_slant_boxes(params);
        if boxes.is_empty() {
            return Vec::new();
        }
        let key_count = self.editor_state.view.key_count as i32;
        let min_ticks = crate::interaction::line_tool::paths::min_note_length_ticks(
            self.editor_state.line_tool.min_note_division,
            self.editor_state.view.ppq,
        );
        let velocity = model.style.velocity.max(1);
        let mut notes = Vec::new();
        match params.fill {
            ShapeFill::Empty => {
                for box_ in &boxes {
                    let outline = funnel_box_outline(box_);
                    notes.extend(
                        self.shape_outline_notes(
                            &outline, track, key_count, min_ticks, false, model,
                        ),
                    );
                }
            }
            ShapeFill::Spam => {
                let gate =
                    (model.style.gate_beats * f32::from(self.editor_state.view.ppq)).max(1.0);
                for box_ in &boxes {
                    for row in funnel_rows(box_, gate, ppq) {
                        if row.key as i32 >= key_count || row.end <= row.start {
                            continue;
                        }
                        notes.push(ShapeNote {
                            track,
                            tick: row.start,
                            key: row.key,
                            length: row.end - row.start,
                            velocity,
                            channel: 0,
                        });
                    }
                }
            }
            ShapeFill::Fill | ShapeFill::OutlineSpam => {}
        }
        notes.sort_by(|a, b| {
            a.key.cmp(&b.key).then(
                a.tick
                    .partial_cmp(&b.tick)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
        });
        let mut deduped: Vec<ShapeNote> = Vec::with_capacity(notes.len());
        for note in notes {
            match deduped.last_mut() {
                Some(last) if last.key == note.key && (last.tick - note.tick).abs() < 0.5 => {
                    if note.length > last.length {
                        *last = note;
                    }
                }
                _ => deduped.push(note),
            }
        }
        if params.reverse {
            deduped.sort_by(|a, b| {
                b.tick
                    .partial_cmp(&a.tick)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        deduped
    }
}
