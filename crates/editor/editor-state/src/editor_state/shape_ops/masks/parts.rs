//! Mask 切分与可见片段：跨边界精确切开，每个碎片都保留（含 1 tick）
//!
//! `shown_parts` 返回的是显示/Apply 用的可见片段；原 notes 不被修改。
//! 未覆盖的形状由调用方保持原样（`preview` 返回 `None`）。

use crate::ShapeNote;

use super::area::Area;
use super::group::UnitArea;

/// 一个可见片段：指向原 notes 的下标 + 切分后的区间
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaskedPiece {
    /// 原 notes 下标
    pub index: usize,
    /// 起点（tick）
    pub start: f32,
    /// 终点（tick）
    pub end: f32,
}

/// 形状是否被遮罩的 reach 触及（至少一条音符与 reach 相交）
pub fn covered(notes: &[ShapeNote], reach: &Area) -> bool {
    notes.iter().any(|note| {
        let start = note.tick;
        let end = start + note.length.max(0.0);
        end > start && reach.intersects(note.key, start, end)
    })
}

/// 计算一个形状的可见片段；`unit.invert` 时取补集
pub fn shown_parts(notes: &[ShapeNote], unit: &UnitArea) -> Vec<MaskedPiece> {
    let mut out = Vec::new();
    for (index, note) in notes.iter().enumerate() {
        let start = note.tick;
        let end = start + note.length.max(0.0);
        if end <= start {
            continue;
        }
        let spans = unit.area.spans_on(note.key);
        if unit.invert {
            let mut cursor = start;
            for span in spans {
                if span.end <= cursor {
                    continue;
                }
                if span.start >= end {
                    break;
                }
                if span.start > cursor {
                    out.push(MaskedPiece {
                        index,
                        start: cursor,
                        end: span.start.min(end),
                    });
                }
                cursor = cursor.max(span.end);
                if cursor >= end {
                    break;
                }
            }
            if cursor < end {
                out.push(MaskedPiece {
                    index,
                    start: cursor,
                    end,
                });
            }
        } else {
            for span in spans {
                let piece_start = start.max(span.start);
                let piece_end = end.min(span.end);
                if piece_end > piece_start {
                    out.push(MaskedPiece {
                        index,
                        start: piece_start,
                        end: piece_end,
                    });
                }
            }
        }
    }
    out
}

/// Apply mask 的音符结果：保留 track/key/velocity，只改 tick/length
pub fn apply_visible(notes: &[ShapeNote], unit: &UnitArea) -> Vec<ShapeNote> {
    shown_parts(notes, unit)
        .into_iter()
        .map(|piece| ShapeNote {
            tick: piece.start,
            length: piece.end - piece.start,
            ..notes[piece.index]
        })
        .collect()
}

/// 预览层：ghost 原音符 + 可见片段；未被触及的形状返回 `None`（保持原样）
#[derive(Debug, Clone, PartialEq)]
pub struct MaskPreview {
    /// 遮罩外/整体 ghost（原音符，供渲染层淡淡画出）
    pub ghosts: Vec<ShapeNote>,
    /// 可见片段（正常绘制）
    pub visible: Vec<ShapeNote>,
}

/// 生成一个形状的预览层
pub fn preview(notes: &[ShapeNote], unit: &UnitArea) -> Option<MaskPreview> {
    if !covered(notes, &unit.reach) {
        return None;
    }
    Some(MaskPreview {
        ghosts: notes.to_vec(),
        visible: apply_visible(notes, unit),
    })
}

#[cfg(test)]
mod tests;
