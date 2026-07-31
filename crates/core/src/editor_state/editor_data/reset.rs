//! 重置方法 —— `reset()` 释放所有内存回初始状态

use super::EditorData;
use super::super::constants::DEFAULT_BPM;
use crate::midi_types::{CcData, TempoPoint};

impl EditorData {
    /// 重置编辑器数据到初始状态（释放所有内存）
    pub fn reset(&mut self) {
        self.notes.clear();
        self.track_notes.clear();
        self.edited_tracks.clear();
        self.mark_track_notes_changed();
        self.current_track = 0;
        self.history.clear();
        self.pending_commit = None;
        self.document = None;
        self.cc_data = CcData::default();
        self.automation_lanes.clear();
        self.tempo_points = vec![TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        }];
        self.time_signatures = vec![(0, 4, 4)];
        self.note_store.clear();
        self.note_store_enabled = false;
        self.arrange_selection.clear();
        self.track_visual_order.clear();
    }
}
