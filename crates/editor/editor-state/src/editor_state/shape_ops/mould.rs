//! Mould 饼干模（DRAW-025）
//!
//! 把同一 key 行上相邻墙音符之间的空隙，按其中点是否在轮廓内筛出来。
//! 短于 `SHORT × 相邻音符长度` 的碎片丢弃。
//!
//! 应用口径（2026-11-11 拍板）：Mould 是**两输入**操作——
//! 刀（`walls`：它的音符当墙）与面团（`dough_notes` + `loops`：音符是原料、
//! 轮廓定边界）。产出「外片」（面团减去空隙）+ 每个空隙连通组一个「内片」；
//! 跨墙音符在墙边界裁剪，短于 `SHORT` 的碎片丢弃。

use std::collections::BTreeMap;

use crate::ShapeNote;

use super::slice::{Pt, inside};

/// 被切掉的碎片短于音符长度的此比例则丢弃
pub const SHORT: f32 = 0.25;

/// 浮点比较容差（tick）
const EPS: f32 = 1e-3;

/// 音符刀 → 逐行墙段 `(key, start, end)`（退化音符直接跳过）
///
/// 几何刀（多环）在 `ui-editor` 侧用同一形态产出墙段，两种来源共用
/// [`mould_rows_from_segments`]，保证「空隙」语义只有一份实现。
pub fn note_spans(walls: &[ShapeNote]) -> Vec<(u16, f32, f32)> {
    let mut out = Vec::with_capacity(walls.len());
    for wall in walls {
        if wall.length <= 0.0 || !wall.tick.is_finite() || !wall.length.is_finite() {
            continue;
        }
        out.push((wall.key, wall.tick, wall.tick + wall.length));
    }
    out
}

/// 每 key 行：墙音符之间、且空隙中点在轮廓内的区间
pub fn mould_rows(walls: &[ShapeNote], loops: &[Vec<Pt>], ppq: u32) -> Vec<(u16, f32, f32)> {
    mould_rows_from_segments(&note_spans(walls), loops, ppq)
}

/// 每 key 行：墙段之间、且空隙中点在轮廓内的区间（音符刀 / 几何刀共用）
///
/// `segments` 为各行的实心墙段；同一行内按起点排序后取相邻段之间的空隙。
pub fn mould_rows_from_segments(
    segments: &[(u16, f32, f32)],
    loops: &[Vec<Pt>],
    _ppq: u32,
) -> Vec<(u16, f32, f32)> {
    let mut by_key: BTreeMap<u16, Vec<(f32, f32)>> = BTreeMap::new();
    for &(key, start, end) in segments {
        if !start.is_finite() || !end.is_finite() || end <= start {
            continue;
        }
        by_key.entry(key).or_default().push((start, end));
    }
    let mut out = Vec::new();
    for (key, mut spans) in by_key {
        spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        for pair in spans.windows(2) {
            let start = pair[0].1;
            let end = pair[1].0;
            if end <= start {
                continue;
            }
            let min_length = pair[0].1 - pair[0].0;
            if end - start < SHORT * min_length {
                continue;
            }
            let mid = (start + end) / 2.0;
            if inside(loops, (mid, key as f32 + 0.5)) {
                out.push((key, start, end));
            }
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    out
}

/// Mould 产出的一片
#[derive(Debug, Clone, PartialEq)]
pub struct MouldPiece {
    /// 是否内片（空隙带）；`false` = 外片（面团剩料）
    pub inner: bool,
    /// 闭合阶梯环（内片 = 空隙带；外片 = 原轮廓）
    pub ring: Vec<Pt>,
    /// 该片的音符（已按墙裁剪、已丢弃短碎片）
    pub notes: Vec<ShapeNote>,
}

/// 两输入 Mould：刀的音符当墙，面团按墙裁剪
///
/// 返回顺序：外片在前，其后是每个空隙连通组的内片。
/// 没有可用空隙时返回空（调用方应报「没有可切的位置」）。
pub fn mould_pieces(
    walls: &[ShapeNote],
    dough_notes: &[ShapeNote],
    loops: &[Vec<Pt>],
    ppq: u32,
) -> Vec<MouldPiece> {
    mould_pieces_from_segments(&note_spans(walls), dough_notes, loops, ppq)
}

/// 两输入 Mould 的共用内核：墙段（音符刀或几何刀）→ 外片 + 内片
pub fn mould_pieces_from_segments(
    segments: &[(u16, f32, f32)],
    dough_notes: &[ShapeNote],
    loops: &[Vec<Pt>],
    ppq: u32,
) -> Vec<MouldPiece> {
    let rows = mould_rows_from_segments(segments, loops, ppq);
    mould_pieces_from_rows(&rows, dough_notes, loops)
}

/// 已算出的空隙区间 → 片；调用方需要切口时复用同一份 `rows`，避免重复扫描
pub fn mould_pieces_from_rows(
    rows: &[(u16, f32, f32)],
    dough_notes: &[ShapeNote],
    loops: &[Vec<Pt>],
) -> Vec<MouldPiece> {
    if rows.is_empty() {
        return Vec::new();
    }
    let groups = group_rows(rows);
    let mut blank: Vec<Pt> = loops.first().cloned().unwrap_or_default();
    // 音符第 k 行占几何 [k-1, k]：轮廓底边上移一行会让最下一行露在框外
    extend_to_bottom_row(&mut blank);
    let mut pieces = Vec::with_capacity(groups.len() + 1);
    // 外片：面团减去全部空隙；几何沿用原轮廓（单环模型表达不了带孔环）
    pieces.push(MouldPiece {
        inner: false,
        ring: blank,
        notes: subtract_rows_from_notes(dough_notes, rows),
    });
    for group in groups {
        pieces.push(MouldPiece {
            inner: true,
            ring: band_ring(&group),
            notes: notes_within_rows(dough_notes, &group),
        });
    }
    pieces
}

/// 把空隙区间按「相邻 key 且区间重叠」合成连通组
fn group_rows(rows: &[(u16, f32, f32)]) -> Vec<Vec<(u16, f32, f32)>> {
    let mut groups: Vec<Vec<(u16, f32, f32)>> = Vec::new();
    for row in rows {
        match groups.last_mut() {
            Some(group)
                if group.last().is_some_and(|last| {
                    last.0 + 1 == row.0 && row.1 < last.2 && row.2 > last.1
                }) =>
            {
                group.push(*row);
            }
            _ => groups.push(vec![*row]),
        }
    }
    groups
}

/// 一组的阶梯闭合环：逐行「竖边 + 水平台阶」，所有边都轴对齐
///
/// 坐标约定：**音符第 `key` 行占屏幕 `[y(key), y(key) + zoom]`**，即几何上的
/// `[key - 1, key]`。早先按 `[key, key + 1]` 建环，整框会比音符高一行。
fn band_ring(rows: &[(u16, f32, f32)]) -> Vec<Pt> {
    let Some((k0, s0, e0)) = rows.first().copied() else {
        return Vec::new();
    };
    let (kn, sn, _) = rows[rows.len() - 1];
    let bottom = k0 as f32 - 1.0;
    let mut ring: Vec<Pt> = Vec::with_capacity(rows.len() * 4 + 2);
    // 下边：左 → 右（落在第 k0 行的底边）
    ring.push((s0, bottom));
    ring.push((e0, bottom));
    // 右边：自下而上，逐行「竖边 + 到下一行的台阶」
    for (index, (key, _, end)) in rows.iter().enumerate() {
        let top = *key as f32;
        ring.push((*end, top));
        if let Some((_, _, next_end)) = rows.get(index + 1) {
            ring.push((*next_end, top));
        }
    }
    // 上边：右 → 左（落在第 kn 行的顶边）
    ring.push((sn, kn as f32));
    // 左边：自上而下，同样逐行「竖边 + 台阶」
    for index in (0..rows.len()).rev() {
        let (key, start, _) = rows[index];
        let bottom = key as f32 - 1.0;
        ring.push((start, bottom));
        if index > 0 {
            let (_, prev_start, _) = rows[index - 1];
            ring.push((prev_start, bottom));
        }
    }
    ring.push((s0, bottom));
    dedup_consecutive(&mut ring);
    ring
}

/// 把轮廓的底边（最小 key）下移一行，保证它包住最下面一行的音符
///
/// 音符第 `key` 行占几何上的 `[key - 1, key]`；形状轮廓原本只到 `key`，
/// 最下一行会露在框外。
fn extend_to_bottom_row(ring: &mut [Pt]) {
    let Some(min_key) = ring.iter().map(|p| p.1).reduce(f32::min) else {
        return;
    };
    for point in ring.iter_mut() {
        if (point.1 - min_key).abs() < 1e-4 {
            point.1 = min_key - 1.0;
        }
    }
}

/// 去掉相邻重复点（台阶与竖边的接缝会重复）
fn dedup_consecutive(points: &mut Vec<Pt>) {
    points.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4);
}

/// 音符与行区间的交集（内片：只保留落在空隙里的部分）
pub fn notes_within_rows(notes: &[ShapeNote], rows: &[(u16, f32, f32)]) -> Vec<ShapeNote> {
    let mut out = Vec::new();
    for note in notes {
        let start = note.tick;
        let end = note.tick + note.length.max(0.0);
        let min_len = (SHORT * note.length).max(EPS);
        for (key, row_start, row_end) in rows {
            if *key != note.key {
                continue;
            }
            let from = start.max(*row_start);
            let to = end.min(*row_end);
            if to - from >= min_len {
                out.push(ShapeNote {
                    tick: from,
                    length: to - from,
                    ..*note
                });
            }
        }
    }
    out.sort_by(|a, b| a.key.cmp(&b.key).then(a.tick.total_cmp(&b.tick)));
    out
}

/// 音符减去行区间（外片：裁掉落在空隙里的部分）
pub fn subtract_rows_from_notes(notes: &[ShapeNote], rows: &[(u16, f32, f32)]) -> Vec<ShapeNote> {
    let mut out = Vec::new();
    for note in notes {
        let start = note.tick;
        let end = note.tick + note.length.max(0.0);
        if end - start <= EPS {
            continue;
        }
        let min_len = (SHORT * note.length).max(EPS);
        let mut holes: Vec<(f32, f32)> = rows
            .iter()
            .filter(|(key, _, _)| *key == note.key)
            .map(|(_, from, to)| (*from, *to))
            .collect();
        holes.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut cursor = start;
        for (hole_start, hole_end) in holes {
            if hole_end <= cursor {
                continue;
            }
            if hole_start > cursor {
                let to = hole_start.min(end);
                if to - cursor >= min_len {
                    out.push(ShapeNote {
                        tick: cursor,
                        length: to - cursor,
                        ..*note
                    });
                }
            }
            cursor = cursor.max(hole_end);
            if cursor >= end {
                break;
            }
        }
        if end - cursor >= min_len {
            out.push(ShapeNote {
                tick: cursor,
                length: end - cursor,
                ..*note
            });
        }
    }
    out.sort_by(|a, b| a.key.cmp(&b.key).then(a.tick.total_cmp(&b.tick)));
    out
}

#[cfg(test)]
mod tests;
