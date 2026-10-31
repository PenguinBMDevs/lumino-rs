//! DRAW-041C 组合结果预览辅助：屏幕包围盒 + 027/028 音符块。
//!
//! 从 `shape_tool_box.rs` 拆出，避免主文件超过 400 行。

use iced_core::Color;
use iced_widget::canvas;
use lumino_editor_state::{ComposedKind, PendingComposed, ShapeNote};
use lumino_ui_core::Renderer;

use crate::Editor;
use crate::grid::utils::fill_note_blocks;

/// 组合结果在文档坐标下的包围盒。
///
/// 027 的副本由 `ShapeOp::CopyAlong` 生成，不在 `model.geometry` 里；
/// 悬浮按钮/预览需要用组合后的音符范围，否则会框不住副本。
pub(super) fn composed_bounds(composed: &PendingComposed) -> Option<(f32, f32, f32, f32)> {
    let mut acc: Option<(f32, f32, f32, f32)> = None;
    for part in &composed.results {
        let bounds = if composed.kind == ComposedKind::CopyAlong {
            notes_bounds(&part.notes).or_else(|| part.model.geometry.bounds())
        } else {
            part.model.geometry.bounds()
        };
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

/// 027/028 的效果不能只靠轮廓表达：直接画组合后的音符块。
///
/// - 027：副本落在路径上，音符块就是 ✓ 会提交的最终位置；
/// - 028：逐音符按力度调制透明度，渐强在提交前可见。
pub(super) fn draw_composed_notes(
    frame: &mut canvas::Frame<Renderer>,
    editor: &Editor,
    composed: &PendingComposed,
    base_color: Color,
) -> bool {
    if composed.notes().next().is_none() {
        return false;
    }
    match composed.kind {
        ComposedKind::CopyAlong => {
            let blocks: Vec<(f32, f32, u16)> = composed.notes().map(note_block).collect();
            fill_note_blocks(
                frame,
                editor,
                &blocks,
                Color {
                    a: 0.55,
                    ..base_color
                },
            )
        }
        ComposedKind::Velocity => {
            let mut drawn = false;
            for note in composed.notes() {
                let alpha = (0.15 + f32::from(note.velocity) / 127.0 * 0.55).clamp(0.15, 0.7);
                drawn |= fill_note_blocks(
                    frame,
                    editor,
                    &[note_block(note)],
                    Color {
                        a: alpha,
                        ..base_color
                    },
                );
            }
            drawn
        }
        _ => false,
    }
}

/// 音符 → `fill_note_blocks` 的 `(tick_lo, tick_hi, key)`。
fn note_block(note: &ShapeNote) -> (f32, f32, u16) {
    (
        note.tick,
        (note.tick + note.length).max(note.tick + 1.0),
        note.key,
    )
}
