use super::PlaybackEngine;
use crate::playback::engine::MidiMessage;
use crate::playback::{Playback, PlaybackState};
use parking_lot::Mutex;
use std::sync::Arc;
use std::time::Duration;

use lumino_midi_loader::{MidiDocument, NoteEvent as DocNoteEvent, TrackManager};

mod loop_wrap;
mod scheduling;
mod track_end;
mod track_rules;

/// 构造单轨（track 0 = 当前轨）文档，避免每个测试重复全字段构造。
/// 当前轨统一从 document 流式读取（2026-08 改造后不再有 set_current_track_notes）。
pub(crate) fn doc_with_current_track(notes: Vec<DocNoteEvent>) -> Arc<MidiDocument> {
    let mut max_end = 0u32;
    for n in &notes {
        max_end = max_end.max(n.end_tick);
    }
    Arc::new(MidiDocument {
        next_note_id: 1,
        notes: vec![lumino_midi_loader::ChunkedList::from_sorted(notes)],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![None],
        total_ticks: max_end,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: lumino_midi_loader::MidiDocument::new_track_max_ticks(1),
    })
}
