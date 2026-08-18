//! 重置方法 —— `reset()` 释放所有内存回初始状态

use super::EditorData;
use lumino_note_core::midi_types::{CcData, TempoPoint};

impl EditorData {
    /// 重置编辑器数据到初始状态（释放所有内存）
    pub fn reset(&mut self) {
        self.edited_tracks.clear();
        self.onion_dirty_tracks = None;
        self.mark_track_notes_changed();
        self.current_track = 0;
        self.history.clear();
        self.pending_commit = None;
        self.document = None;
        self.cc_data = CcData::default();
        self.automation_lanes.clear();
        self.set_tempo_points(vec![TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        }]);
        self.time_signatures = vec![(0, 4, 4)];
        self.key_signatures.clear();
        self.markers.clear();
        self.lyrics.clear();
        self.chords.clear();
        self.program_changes.clear();
        self.arrange_selection.clear();
        // 主音轨增量对账：重置 = 全量重建（事件队列不可信）
        self.note_delta_events.clear();
        self.note_delta_dirty = true;
        self.track_visual_order.clear();
    }
}
