//! 直接编辑索引区间 → 脏 tick 区间。
//!
//! 历史快照只保存被编辑轨的旧值；这里按索引把旧/新音符的 tick 跨度并起来，
//! 供 undo/redo 局部重建密度，不扫全文档。

use lumino_midi_model::{ChunkedList, NoteEvent};

use crate::editor_state::editor_data::cpu_onion_dirty::push_dirty_range;

/// 把 `(track, index_lo, index_hi)` 索引区间翻成 tick 区间。
///
/// `previous` 为空表示该轨不在历史快照里：只用当前值（仍不扫全文档）。
pub(super) fn direct_ranges(
    previous: Option<&ChunkedList<NoteEvent>>,
    current: &ChunkedList<NoteEvent>,
    ranges: &[(usize, usize, usize)],
) -> Vec<(u32, u32)> {
    let mut out: Vec<(u32, u32)> = Vec::new();
    for &(track, lo, hi) in ranges {
        let _ = track;
        for index in lo..=hi {
            let old = previous.and_then(|notes| notes.get(index)).copied();
            let new = current.get(index).copied();
            for note in [old, new].into_iter().flatten() {
                let end = note.end_tick.max(note.start_tick.saturating_add(1));
                push_union(&mut out, note.start_tick, end);
            }
        }
    }
    out
}

pub(super) fn push_union(out: &mut Vec<(u32, u32)>, start: u32, end: u32) {
    push_dirty_range(out, start, end);
}
