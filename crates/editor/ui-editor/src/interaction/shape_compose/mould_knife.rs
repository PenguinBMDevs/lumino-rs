//! 饼干模的刀（DEBT-17）：音符刀 / 几何刀
//!
//! 刀是**显式会话**：由「以选中内容为刀」设置，切完 / 失效 / 换轨 / 显式清除时结束；
//! 不再依赖「先选音符再切工具」的顺序。
//!
//! 两种来源最终都归一到「逐 key 行墙段」，与
//! [`lumino_editor_state::shape_ops::mould_rows_from_segments`] 共用同一套空隙语义：
//! - 音符刀：音符本身是墙；
//! - 几何刀：待确认形状的闭环按**奇偶规则**合并（嵌套环 = 孔，独立环 = 并集）。

use std::collections::BTreeMap;

use lumino_editor_state::ShapeNote;
use lumino_editor_state::shape_ops::{Pt, note_spans, ring_of};

use crate::Editor;
use crate::interaction::line_tool::fill::spans::row_spans;

/// 刀失效通知（UI 层取走后转 toast）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouldNotice {
    /// 刀源被删 / 改 / 换轨，刀已失效并被清除
    Expired,
}

/// 设刀结果摘要（toast / 覆盖层状态用）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouldKnifeInfo {
    /// 音符刀条数；几何刀为 `None`
    pub notes: Option<usize>,
    /// 刀覆盖的 key 行数
    pub rows: u16,
    /// 几何刀环数；音符刀为 `0`
    pub rings: usize,
    /// 是否至少有一行存在两段墙（否则切不出内片）
    pub can_cut: bool,
}

/// 饼干模的刀
#[derive(Debug, Clone, PartialEq)]
pub enum MouldKnife {
    /// 音符刀：当前轨的手画音符
    Notes {
        /// 设刀时所在音轨
        track: usize,
        /// 墙音符
        notes: Vec<ShapeNote>,
    },
    /// 几何刀：待确认形状的闭环
    Geometry {
        /// 闭环（首尾重复）
        rings: Vec<Vec<Pt>>,
    },
}

impl MouldKnife {
    /// 音符刀的音符（几何刀为空）
    pub fn notes(&self) -> &[ShapeNote] {
        match self {
            Self::Notes { notes, .. } => notes,
            Self::Geometry { .. } => &[],
        }
    }

    /// 几何刀的环（音符刀为空）
    pub fn rings(&self) -> &[Vec<Pt>] {
        match self {
            Self::Notes { .. } => &[],
            Self::Geometry { rings } => rings,
        }
    }

    /// 摘要（条数 / 行数 / 环数 / 是否可切）
    pub fn info(&self) -> MouldKnifeInfo {
        match self {
            Self::Notes { notes, .. } => {
                let mut keys: Vec<u16> = notes.iter().map(|note| note.key).collect();
                keys.sort_unstable();
                keys.dedup();
                MouldKnifeInfo {
                    notes: Some(notes.len()),
                    rows: keys.len().min(u16::MAX as usize) as u16,
                    rings: 0,
                    can_cut: has_cut_gap(&note_spans(notes)),
                }
            }
            Self::Geometry { rings } => {
                let (key_lo, key_hi) = ring_key_range(rings);
                MouldKnifeInfo {
                    notes: None,
                    rows: ring_row_count(rings),
                    rings: rings.len(),
                    can_cut: has_cut_gap(&ring_wall_segments(rings, key_lo, key_hi)),
                }
            }
        }
    }

    /// 逐 key 行墙段（限制在面团的 key 范围内）
    pub fn wall_segments(&self, key_lo: u16, key_hi: u16) -> Vec<(u16, f32, f32)> {
        match self {
            Self::Notes { notes, .. } => note_spans(notes)
                .into_iter()
                .filter(|(key, _, _)| *key >= key_lo && *key <= key_hi)
                .collect(),
            Self::Geometry { rings } => ring_wall_segments(rings, key_lo, key_hi),
        }
    }

    /// 刀是否仍与当前文档 / 待确认几何一致
    ///
    /// 不一致即「过期刀」：调用方必须清除并提示，不能静默沿用旧几何。
    pub fn is_current(&self, editor: &Editor) -> bool {
        match self {
            Self::Notes { track, notes } => {
                if *track != editor.editor_state.data.current_track {
                    return false;
                }
                let current = editor.editor_state.data.track_notes(*track);
                notes.iter().all(|knife| {
                    current.iter().any(|note| {
                        note.start_tick as f32 == knife.tick
                            && u16::from(note.key) == knife.key
                            && (note.end_tick.saturating_sub(note.start_tick)) as f32
                                == knife.length
                    })
                })
            }
            Self::Geometry { rings } => {
                let current = pending_rings(editor);
                rings.iter().all(|ring| {
                    current
                        .iter()
                        .any(|current_ring| ring_equals(current_ring, ring))
                })
            }
        }
    }

    /// 几何刀的源被删/改后立即失效（音符刀由事件路径 `sync_mould_knife` 收口）
    pub fn is_geometry_stale(&self, editor: &Editor) -> bool {
        matches!(self, Self::Geometry { .. }) && !self.is_current(editor)
    }

    /// 几何刀里是否已包含某个环（防止「刀与面团是同一个形状」）
    pub fn contains_ring(&self, ring: &[Pt]) -> bool {
        self.rings()
            .iter()
            .any(|knife_ring| ring_equals(knife_ring, ring))
    }
}
/// 当前选中待确认形状的闭环（设刀入口）
pub(crate) fn selected_pending_rings(editor: &Editor) -> Vec<Vec<Pt>> {
    let track = editor.editor_state.data.current_track;
    editor
        .editor_state
        .shape_select
        .selected_shapes()
        .filter(|shape| shape.pending.is_some())
        .filter_map(|shape| shape.pending)
        .filter_map(|reference| editor.pending_source(reference))
        .map(|source| ring_of(&editor.model_from_source_at_view(&source, track, 1.0)))
        .filter(|ring| !ring.is_empty())
        .collect()
}

/// 全部待确认形状的闭环（失效判定）
fn pending_rings(editor: &Editor) -> Vec<Vec<Pt>> {
    let track = editor.editor_state.data.current_track;
    editor
        .editor_state
        .shape_select
        .pending_shapes()
        .filter_map(|(_, reference)| editor.pending_source(reference))
        .map(|source| ring_of(&editor.model_from_source_at_view(&source, track, 1.0)))
        .filter(|ring| !ring.is_empty())
        .collect()
}

/// 两个闭环是否逐点一致（容差 1e-3）
pub(crate) fn ring_equals(a: &[Pt], b: &[Pt]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b.iter())
            .all(|(p, q)| (p.0 - q.0).abs() < 1e-3 && (p.1 - q.1).abs() < 1e-3)
}

/// 墙段里是否至少有一行存在两段（中间才有空隙）
fn has_cut_gap(segments: &[(u16, f32, f32)]) -> bool {
    let mut by_key: BTreeMap<u16, Vec<(f32, f32)>> = BTreeMap::new();
    for &(key, start, end) in segments {
        if end > start {
            by_key.entry(key).or_default().push((start, end));
        }
    }
    for spans in by_key.values_mut() {
        spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        if spans.windows(2).any(|pair| pair[1].0 > pair[0].1 + 1e-6) {
            return true;
        }
    }
    false
}

/// 环的整数 key 范围（空环返回 `0..=0`）
fn ring_key_range(rings: &[Vec<Pt>]) -> (u16, u16) {
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for point in rings.iter().flatten() {
        lo = lo.min(point.1);
        hi = hi.max(point.1);
    }
    if !lo.is_finite() || !hi.is_finite() || hi < lo {
        return (0, 0);
    }
    (
        lo.floor().clamp(0.0, u16::MAX as f32) as u16,
        hi.ceil().clamp(0.0, u16::MAX as f32) as u16,
    )
}

/// 环覆盖的整数 key 行数（按纵坐标包络估算，覆盖层状态用）
fn ring_row_count(rings: &[Vec<Pt>]) -> u16 {
    let mut lo = f32::INFINITY;
    let mut hi = f32::NEG_INFINITY;
    for point in rings.iter().flatten() {
        lo = lo.min(point.1);
        hi = hi.max(point.1);
    }
    if !lo.is_finite() || !hi.is_finite() || hi < lo {
        return 0;
    }
    // 音符第 k 行占几何 [k-1, k]：几何高度即覆盖的行数
    ((hi - lo).ceil().max(0.0) as u32).min(u16::MAX as u32) as u16
}

/// 多环逐行墙段：每环先取行区间，再按奇偶规则合并
fn ring_wall_segments(rings: &[Vec<Pt>], key_lo: u16, key_hi: u16) -> Vec<(u16, f32, f32)> {
    let mut out = Vec::new();
    for key in key_lo..=key_hi {
        let q = key as f32;
        let mut spans: Vec<(f32, f32)> = Vec::new();
        for ring in rings {
            spans.extend(row_spans(ring, q));
        }
        for (start, end) in odd_spans(&spans) {
            out.push((key, start, end));
        }
    }
    out
}

/// 一维区间的奇偶合并：被奇数个区间覆盖的原子段保留
///
/// 与填充引擎的 even-odd 规则一致：嵌套环抵消成孔，独立环退化为并集。
fn odd_spans(spans: &[(f32, f32)]) -> Vec<(f32, f32)> {
    let mut events: Vec<(f32, i32)> = Vec::with_capacity(spans.len() * 2);
    for &(start, end) in spans {
        if !start.is_finite() || !end.is_finite() || end <= start {
            continue;
        }
        events.push((start, 1));
        events.push((end, -1));
    }
    if events.is_empty() {
        return Vec::new();
    }
    events.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
    let mut out = Vec::new();
    let mut parity = 0i32;
    let mut start = 0.0f32;
    for (x, _delta) in events {
        let before = parity;
        parity ^= 1;
        if before == 0 && parity == 1 {
            start = x;
        } else if before == 1 && parity == 0 && x > start {
            out.push((start, x));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<Pt> {
        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)]
    }

    fn note(tick: f32, key: u16, length: f32) -> ShapeNote {
        ShapeNote {
            track: 1,
            tick,
            key,
            length,
            velocity: 100,
            channel: 0,
        }
    }

    #[test]
    fn test_info_reports_can_cut() {
        let gap = MouldKnife::Notes {
            track: 1,
            notes: vec![note(0.0, 60, 100.0), note(200.0, 60, 100.0)],
        };
        assert!(gap.info().can_cut, "同一行两段墙应可切");

        let single = MouldKnife::Notes {
            track: 1,
            notes: vec![note(0.0, 60, 100.0)],
        };
        assert!(!single.info().can_cut, "每行只有一段墙切不出内片");

        let nested = MouldKnife::Geometry {
            rings: vec![rect(0.0, 59.0, 100.0, 63.0), rect(30.0, 60.0, 70.0, 62.0)],
        };
        assert!(nested.info().can_cut, "嵌套环应可切出孔");

        let solid = MouldKnife::Geometry {
            rings: vec![rect(0.0, 59.0, 100.0, 63.0)],
        };
        assert!(!solid.info().can_cut, "单实心环切不出内片");
    }

    #[test]
    fn test_odd_spans_handles_nested_and_disjoint() {
        // 嵌套：外 0..10、内 3..7 → 0..3 + 7..10
        assert_eq!(
            odd_spans(&[(0.0, 10.0), (3.0, 7.0)]),
            vec![(0.0, 3.0), (7.0, 10.0)]
        );
        // 独立：0..4 + 10..14 → 两段都保留
        assert_eq!(
            odd_spans(&[(0.0, 4.0), (10.0, 14.0)]),
            vec![(0.0, 4.0), (10.0, 14.0)]
        );
    }

    #[test]
    fn test_ring_wall_segments_keeps_hole_as_gap() {
        // 外环 key 59..63、内环 key 60..62 → key 61 行出现两段墙（中间是孔）
        let rings = vec![rect(0.0, 59.0, 100.0, 63.0), rect(30.0, 60.0, 70.0, 62.0)];
        let segments = ring_wall_segments(&rings, 61, 61);
        assert_eq!(
            segments.len(),
            2,
            "带孔环在同一行应产生两段墙：{segments:?}"
        );
        assert!((segments[0].1 - 0.0).abs() < 1e-3);
        assert!((segments[0].2 - 30.0).abs() < 1e-3);
        assert!((segments[1].1 - 70.0).abs() < 1e-3);
    }

    #[test]
    fn test_ring_row_count_covers_bbox_rows() {
        let rings = vec![rect(0.0, 59.0, 100.0, 63.0)];
        assert_eq!(ring_row_count(&rings), 4, "59..=62 共 4 行");
    }
}
