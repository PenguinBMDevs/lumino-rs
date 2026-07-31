//! 构造方法 —— `new()` + `Default`

use std::collections::{HashMap, HashSet};

use super::EditorData;
use super::super::constants::DEFAULT_BPM;
use crate::arrange_selection::ArrangeSelection;
use crate::history::History;
use crate::midi_types::{CcData, TempoPoint};
use crate::note_store::NoteStore;

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
            note_store: NoteStore::new(),
            note_store_enabled: false,
            note_store_dirty: false,
            arrange_selection: ArrangeSelection::new(),
            track_visual_order: Vec::new(),
        }
    }
}
