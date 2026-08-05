//! 构造方法 —— `new()` + `Default`

use std::collections::{HashMap, HashSet};

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
            notes: im::Vector::new(),
            current_track: 0,
            track_notes: HashMap::new(),
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
}
