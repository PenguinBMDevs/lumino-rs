//! 构造方法 —— `new()` + `Default`

use std::collections::HashSet;

use super::super::constants::DEFAULT_BPM;
use super::EditorData;
use lumino_note_core::arrange_selection::ArrangeSelection;
use lumino_note_core::history::History;
use lumino_note_core::midi_types::{CcData, TempoPoint};

impl Default for EditorData {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorData {
    /// 创建新的编辑器数据实例
    pub fn new() -> Self {
        Self {
            current_track: 0,
            track_notes_gen: 0,
            edited_tracks: HashSet::new(),
            onion_dirty_tracks: None,
            document: None,
            history: History::new(),
            pending_commit: None,
            cc_data: CcData::default(),
            automation_lanes: Vec::new(),
            tempo_points: vec![TempoPoint {
                tick: 0.0,
                bpm: DEFAULT_BPM,
            }],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: Vec::new(),
            markers: Vec::new(),
            lyrics: Vec::new(),
            chords: Vec::new(),
            program_changes: Vec::new(),
            arrange_selection: ArrangeSelection::new(),
            note_delta_events: Vec::new(),
            note_delta_dirty: false,
            track_visual_order: Vec::new(),
        }
    }

    /// 测试辅助：创建含指定音轨音符的 EditorData（当前轨 = track_id）
    ///
    /// 构造一个 `MidiDocument` 并填充第 `track_id` 轨的音符，
    /// 音符按 start_tick 升序插入。测试专用，避免每个测试手工构造 document。
    #[cfg(test)]
    pub fn with_notes(track_id: usize, notes: &[lumino_midi_model::NoteEvent]) -> Self {
        let mut data = Self::new();
        data.current_track = track_id;
        let mut doc = lumino_midi_model::MidiDocument {
            notes: (0..=track_id)
                .map(|_| lumino_midi_model::ChunkedList::new())
                .collect(),
            tempo_changes: vec![(0, 120.0)],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: vec![],
            control_events: lumino_midi_model::ChunkedList::new(),
            lyrics: vec![],
            markers: vec![],
            sys_ex: vec![],
            track_names: (0..=track_id).map(|i| Some(format!("Track {i}"))).collect(),
            total_ticks: 0,
            track_count: (track_id + 1) as u16,
            tracks: lumino_midi_model::TrackManager::new((track_id + 1) as u16),
            division: 480,
            track_ports: vec![0; track_id + 1],

            track_max_end_ticks: lumino_midi_model::MidiDocument::new_track_max_ticks(track_id + 1),
        };
        for note in notes {
            doc.insert_note(track_id, *note);
        }
        data.document = Some(doc);
        data
    }

    /// 测试辅助：用 f32 Note 列表构造 EditorData（当前轨 = track_id）
    #[cfg(test)]
    pub fn with_f32_notes(track_id: usize, notes: &[lumino_note_core::note::Note]) -> Self {
        let events: Vec<lumino_midi_model::NoteEvent> = notes
            .iter()
            .map(|n| super::accessors::note_to_event(n.clone()))
            .collect();
        Self::with_notes(track_id, &events)
    }
}
