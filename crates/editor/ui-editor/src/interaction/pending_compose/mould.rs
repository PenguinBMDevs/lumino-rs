//! Mould 组合的构件（DEBT-17）
//!
//! 刀（音符 / 几何）→ 逐行墙段 → 空隙区间 → 外片 + 内片。
//! 空隙区间同时作为切口返回，刀清空后由 `PendingComposed::mould_cuts` 保留。

use lumino_core::shape::ShapeModel;
use lumino_editor_state::shape_ops::{
    CutRef, KnifeKind, mould_pieces_from_rows, mould_rows_from_segments, ring_of,
};
use lumino_editor_state::{ComposedPart, ShapeNote};

use super::slice::polyline_model;
use crate::interaction::shape_compose::MouldKnife;

/// 组合结果 + 切口区间
type MouldOutput = (Vec<ComposedPart>, Vec<(u16, f32, f32)>);

/// 单面团 + 刀 → 组合结果与切口区间；没有可用空隙时返回 `None`
pub(super) fn compose_mould(
    knife: &MouldKnife,
    dough: &ShapeModel,
    dough_notes: &[ShapeNote],
    track: usize,
    ppq: u32,
) -> Option<MouldOutput> {
    let loops = vec![ring_of(dough)];
    let (key_lo, key_hi) = note_key_bounds(dough_notes)?;
    let segments = knife.wall_segments(key_lo, key_hi);
    let rows = mould_rows_from_segments(&segments, &loops, ppq);
    let pieces = mould_pieces_from_rows(&rows, dough_notes, &loops);
    if pieces.is_empty() {
        return None;
    }
    let parts = pieces
        .into_iter()
        .map(|piece| {
            let model = polyline_model(vec![piece.ring], track, dough.k, true);
            let mut part = ComposedPart::plain(model, piece.notes);
            let mut cut = CutRef::for_piece(dough.clone(), &part.model, (0.0, None));
            cut.knife = Some(KnifeKind::Mould);
            part.cut = Some(cut);
            part
        })
        .collect();
    Some((parts, rows))
}

/// 音符覆盖的 key 范围（面团的行范围；空集合返回 `None`）
fn note_key_bounds(notes: &[ShapeNote]) -> Option<(u16, u16)> {
    let mut iter = notes.iter().map(|note| note.key);
    let first = iter.next()?;
    Some(iter.fold((first, first), |(lo, hi), key| (lo.min(key), hi.max(key))))
}
