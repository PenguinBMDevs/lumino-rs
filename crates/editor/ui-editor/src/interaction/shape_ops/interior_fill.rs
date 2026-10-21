//! Nested / Tiles 内部填充接线（DRAW-017）
//!
//! 在普通 Fill / Spam 生成之后、Notes 算子之前消费：
//! 用已生成音符的逐行 span 作为区域，产出带通道的 `ShapeNote`。
//! `of` 尚未有形状库，本卡先引用当前形状自身。

use lumino_core::shape::{ShapeModel, ShapeOp};
use lumino_editor_state::ShapeNote;
use lumino_editor_state::shape_ops::{RowSpan, nested_spans, tiled_spans};

use crate::Editor;

impl Editor {
    /// 应用最后一个 Nested / Tiles（两者同时存在时后者优先）
    pub(super) fn apply_interior_fill(
        &self,
        notes: Vec<ShapeNote>,
        model: &ShapeModel,
        track: usize,
        ppq: f64,
    ) -> Vec<ShapeNote> {
        let Some(op) = model
            .ops
            .iter()
            .rev()
            .find(|op| matches!(op, ShapeOp::Nested(_) | ShapeOp::Tiles(_)))
        else {
            return notes;
        };
        let area = area_of(&notes);
        if area.is_empty() {
            return notes;
        }
        let velocity = model.style.velocity.max(1);
        match op {
            ShapeOp::Nested(params) => {
                let drawn_k = f64::from(model.k) * ppq;
                let spans = nested_spans(&area, params, drawn_k as f32);
                spans_to_notes(&spans, track, velocity)
            }
            ShapeOp::Tiles(params) => {
                let Some(tile) = tile_of(&notes, params.length, params.keys) else {
                    return notes;
                };
                let spans = tiled_spans(&area, &tile, params);
                spans_to_notes(&spans, track, velocity)
            }
            _ => notes,
        }
    }
}

fn area_of(notes: &[ShapeNote]) -> Vec<RowSpan> {
    notes
        .iter()
        .filter(|note| note.length > 0.0)
        .map(|note| RowSpan {
            key: note.key,
            start: note.tick,
            end: note.tick + note.length,
        })
        .collect()
}

/// 当前形状自身归一化成一块 tile（0..length × 0..keys）
fn tile_of(notes: &[ShapeNote], length: f32, keys: f32) -> Option<Vec<RowSpan>> {
    let mut min_tick = f32::INFINITY;
    let mut max_tick = f32::NEG_INFINITY;
    let mut min_key = u16::MAX;
    let mut max_key = 0u16;
    for note in notes {
        if note.length <= 0.0 {
            continue;
        }
        min_tick = min_tick.min(note.tick);
        max_tick = max_tick.max(note.tick + note.length);
        min_key = min_key.min(note.key);
        max_key = max_key.max(note.key);
    }
    if !min_tick.is_finite() || max_tick <= min_tick || min_key == u16::MAX {
        return None;
    }
    let width = max_tick - min_tick;
    let height = f32::from(max_key.saturating_sub(min_key)).max(1.0);
    let mut tile = Vec::new();
    for note in notes {
        if note.length <= 0.0 {
            continue;
        }
        let start = (note.tick - min_tick) / width * length;
        let end = (note.tick + note.length - min_tick) / width * length;
        let key = ((f32::from(note.key.saturating_sub(min_key)) / height) * keys).round();
        if end > start && key.is_finite() {
            tile.push(RowSpan {
                key: key.clamp(0.0, (keys - 1.0).max(0.0)) as u16,
                start,
                end,
            });
        }
    }
    (!tile.is_empty()).then_some(tile)
}

fn spans_to_notes(
    spans: &[lumino_editor_state::shape_ops::ColouredSpan],
    track: usize,
    velocity: u8,
) -> Vec<ShapeNote> {
    spans
        .iter()
        .filter(|span| span.end > span.start)
        .map(|span| ShapeNote {
            track,
            tick: span.start,
            key: span.key,
            length: span.end - span.start,
            velocity,
            channel: span.colour.min(15),
        })
        .collect()
}
