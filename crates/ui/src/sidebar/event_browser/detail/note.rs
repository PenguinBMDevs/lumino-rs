//! 事件浏览器表格行聚合 — 音符事件。

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use lumino_note_core::note::{Note, note_name};
use lumino_ui_core::sidebar_event::{EditRequest, NoteRef};

use crate::sidebar::event_browser::bar_lookup::BarLookup;
use crate::sidebar::event_browser::detail::{EventBrowserData, EventTableRow, make_jump};

/// 收集音符事件行。
pub(super) fn collect_note_rows(
    data: &EventBrowserData<'_>,
    bl: &BarLookup,
    track: u16,
) -> Vec<EventTableRow> {
    // 当前实现仅显示 current_track_notes；未来可扩展为按 track 索引取对应音符集合。
    let _ = track;
    data.current_track_notes
        .iter()
        .enumerate()
        .map(|(idx, note)| {
            let start_tick = note.tick as u32;
            let end_tick = (note.tick + note.length) as u32;
            let note_ref = note_ref(note, track);
            let key_name = note_name(note.key);
            let cells = vec![
                String::new(),
                note_ref.id.to_string(),
                start_tick.to_string(),
                bl.format(start_tick),
                format!("{:.2}", note.length),
                end_tick.to_string(),
                bl.format(end_tick),
                key_name,
                note.velocity.to_string(),
                note.channel.to_string(),
            ];
            let edits = vec![
                None,
                None,
                Some(EditRequest::NoteStartTick { note: note_ref }),
                Some(EditRequest::NoteStartTick { note: note_ref }),
                Some(EditRequest::NoteGate { note: note_ref }),
                Some(EditRequest::NoteEndTick { note: note_ref }),
                Some(EditRequest::NoteEndTick { note: note_ref }),
                Some(EditRequest::NoteKey { note: note_ref }),
                Some(EditRequest::NoteVelocity { note: note_ref }),
                None,
            ];
            let note_jump = make_jump(start_tick, Some((track, note.key as u8)));
            let jumps = vec![
                None,
                None,
                note_jump.clone(),
                note_jump.clone(),
                note_jump.clone(),
                note_jump.clone(),
                note_jump.clone(),
                note_jump.clone(),
                note_jump.clone(),
                note_jump,
            ];
            EventTableRow {
                id: idx,
                tick: start_tick,
                cells,
                cell_edits: edits,
                cell_jumps: jumps,
            }
        })
        .collect()
}

/// 构造音符引用：以字段哈希作为稳定 id。
fn note_ref(note: &Note, track: u16) -> NoteRef {
    let mut hasher = DefaultHasher::new();
    note.tick.to_bits().hash(&mut hasher);
    note.key.hash(&mut hasher);
    note.length.to_bits().hash(&mut hasher);
    note.velocity.hash(&mut hasher);
    note.channel.hash(&mut hasher);
    track.hash(&mut hasher);
    NoteRef {
        id: hasher.finish(),
        start_tick: note.tick as u32,
        end_tick: (note.tick + note.length) as u32,
        key: note.key as u8,
        velocity: note.velocity,
        track,
    }
}
