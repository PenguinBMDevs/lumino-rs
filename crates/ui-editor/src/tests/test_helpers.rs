//! 测试辅助：单一权威源改造后，测试种子必须写入 document
//!
//! 2026-08 单一权威源：音符唯一权威是 `document`（MidiDocument），
//! `EditorData::notes` / `track_notes` 缓存已删除。editor-state 的
//! `with_notes` / `with_f32_notes` 是 `#[cfg(test)]` 辅助，本 crate（ui-editor）
//! 作为依赖方无法调用，因此在此自建等价的 document 构造辅助。

use crate::Editor;
use crate::note::Note;
use lumino_midi_loader::{MidiDocument, NoteEvent, TrackManager};

/// 构造一个 `MidiDocument`，在第 `track_id` 轨写入 `notes`（按 start_tick 升序）
///
/// `track_count` 会与 `track_id + 1` 取 max，保证目标音轨存在。
pub(crate) fn doc_with_notes(track_count: usize, track_id: usize, notes: &[Note]) -> MidiDocument {
    let track_count = track_count.max(track_id + 1);
    let mut doc = MidiDocument {
        notes: (0..track_count)
            .map(|_| lumino_midi_loader::ChunkedList::new())
            .collect(),
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: (0..track_count)
            .map(|i| Some(format!("Track {i}")))
            .collect(),
        total_ticks: 0,
        track_count: track_count as u16,
        tracks: TrackManager::new(track_count as u16),
        division: 480,
        track_ports: vec![0; track_count],
    };
    let mut events: Vec<NoteEvent> = notes
        .iter()
        .map(|n| {
            NoteEvent::new(
                n.tick.round() as u32,
                (n.tick + n.length).round() as u32,
                n.key as u8,
                n.velocity,
                n.channel,
            )
        })
        .collect();
    // 与 MidiDocument::insert_note 一致：保持每轨 start_tick 升序不变式
    events.sort_by_key(|e| e.start_tick);
    doc.notes[track_id] = lumino_midi_loader::ChunkedList::from_sorted(events);
    doc
}

/// 将音符种子写入 `Editor` 的 document（当前轨 = track_id）
pub(crate) fn seed_notes(editor: &mut Editor, track_count: usize, track_id: usize, notes: &[Note]) {
    editor.editor_state.data.document = Some(doc_with_notes(track_count, track_id, notes));
    editor.editor_state.data.current_track = track_id;
}

/// 便捷版：单轨种子（track_count = 1，当前轨 = 0）
#[allow(dead_code)]
pub(crate) fn seed_single_track(editor: &mut Editor, notes: &[Note]) {
    seed_notes(editor, 1, 0, notes);
}
