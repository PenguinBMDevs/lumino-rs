//! Glue 音符算法：同 key、同 track 上相接/重叠的音符合并成一条
//!
//! 与现有 `glue_selected_notes` 同源但**不复用**：那是用户选区的一次性动作，
//! 本模块是形状再生成链路里的纯函数，gap 规则固定为「< 1 tick 粘连，= 1 tick 断开」。

use std::cmp::Ordering;

use lumino_core::shape::{GlueParams, ShapeRect};

use crate::ShapeNote;

const EPS: f32 = 1e-6;
const GAP: f32 = 1.0;

/// 应用 Glue；`shape_box` 为形状包围盒（Boxes 的相对坐标系）
pub fn apply(
    notes: &[ShapeNote],
    params: &GlueParams,
    shape_box: Option<ShapeRect>,
) -> Vec<ShapeNote> {
    if notes.len() < 2 {
        return notes.to_vec();
    }
    let boxes = match params {
        GlueParams::All => None,
        GlueParams::Boxes(boxes) => {
            if boxes.is_empty() {
                return notes.to_vec();
            }
            let Some(shape_box) = shape_box else {
                return notes.to_vec();
            };
            Some((boxes.as_slice(), shape_box))
        }
    };

    // 稳定排序：(track, key, tick, 原始索引)
    let mut ordered: Vec<(usize, ShapeNote)> = notes.iter().copied().enumerate().collect();
    ordered.sort_by(|(ia, a), (ib, b)| {
        a.track
            .cmp(&b.track)
            .then(a.key.cmp(&b.key))
            .then(a.tick.partial_cmp(&b.tick).unwrap_or(Ordering::Equal))
            .then(ia.cmp(ib))
    });

    let mut out = Vec::with_capacity(notes.len());
    let mut group: Option<Group> = None;
    for (_, note) in ordered {
        let candidate = boxes.is_none_or(|(boxes, box_)| in_boxes(&note, boxes, &box_));
        if !candidate {
            flush(&mut group, &mut out);
            out.push(note);
            continue;
        }
        let end = note.tick + note.length.max(0.0);
        match group.as_mut() {
            Some(current)
                if current.first.track == note.track
                    && current.first.key == note.key
                    && note.tick < current.max_end + GAP =>
            {
                current.max_end = current.max_end.max(end);
                current.count += 1;
            }
            _ => {
                flush(&mut group, &mut out);
                group = Some(Group {
                    first: note,
                    max_end: end,
                    count: 1,
                });
            }
        }
    }
    flush(&mut group, &mut out);
    out
}

struct Group {
    first: ShapeNote,
    max_end: f32,
    count: usize,
}

fn flush(group: &mut Option<Group>, out: &mut Vec<ShapeNote>) {
    let Some(group) = group.take() else {
        return;
    };
    if group.count == 1 {
        out.push(group.first);
        return;
    }
    out.push(ShapeNote {
        track: group.first.track,
        tick: group.first.tick,
        key: group.first.key,
        length: (group.max_end - group.first.tick).max(0.0),
        velocity: group.first.velocity,
    });
}

fn in_boxes(note: &ShapeNote, boxes: &[[f32; 4]], shape: &ShapeRect) -> bool {
    let width = shape.tick_hi - shape.tick_lo;
    let height = shape.key_hi - shape.key_lo;
    if width.abs() <= EPS || height.abs() <= EPS {
        return false;
    }
    let mid = note.tick + note.length.max(0.0) * 0.5;
    let u = (mid - shape.tick_lo) / width;
    let v = (note.key as f32 - shape.key_lo) / height;
    boxes.iter().any(|corners| {
        let (u0, u1) = if corners[0] <= corners[2] {
            (corners[0], corners[2])
        } else {
            (corners[2], corners[0])
        };
        let (v0, v1) = if corners[1] <= corners[3] {
            (corners[1], corners[3])
        } else {
            (corners[3], corners[1])
        };
        u >= u0 - EPS && u <= u1 + EPS && v >= v0 - EPS && v <= v1 + EPS
    })
}

#[cfg(test)]
mod tests;
