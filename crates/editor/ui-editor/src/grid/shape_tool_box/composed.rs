//! DRAW-041C 组合结果预览辅助：屏幕包围盒 + 027/028 音符块。
//!
//! 从 `shape_tool_box.rs` 拆出，避免主文件超过 400 行。

use lumino_editor_state::{PendingComposed, ShapeNote};

/// 组合结果在文档坐标下的包围盒。
///
/// 027 的副本由 `ShapeOp::CopyAlong` 生成，不在 `model.geometry` 里；
/// 悬浮按钮/预览需要用组合后的音符范围，否则会框不住副本。
pub(super) fn composed_bounds(composed: &PendingComposed) -> Option<(f32, f32, f32, f32)> {
    let mut acc: Option<(f32, f32, f32, f32)> = None;
    for part in &composed.results {
        // 023 半边对称 / 027 副本都在生成阶段展开，不在 `model.geometry` 里；
        // 预览按钮必须框住「最终会生成的音符」，否则只框住半边。
        let bounds = notes_bounds(&part.notes).or_else(|| part.model.geometry.bounds());
        let Some(b) = bounds else {
            continue;
        };
        acc = Some(match acc {
            None => b,
            Some(a) => (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
        });
    }
    acc
}

fn notes_bounds(notes: &[ShapeNote]) -> Option<(f32, f32, f32, f32)> {
    let mut lo_t = f32::INFINITY;
    let mut hi_t = f32::NEG_INFINITY;
    let mut lo_k = f32::INFINITY;
    let mut hi_k = f32::NEG_INFINITY;
    for note in notes {
        lo_t = lo_t.min(note.tick);
        hi_t = hi_t.max(note.tick + note.length.max(0.0));
        lo_k = lo_k.min(f32::from(note.key));
        hi_k = hi_k.max(f32::from(note.key));
    }
    (lo_t.is_finite() && hi_t.is_finite()).then_some((lo_t, lo_k, hi_t, hi_k))
}
