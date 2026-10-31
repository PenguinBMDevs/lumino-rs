//! 重置方法 —— `reset()` 释放所有内存回初始状态

use super::EditorData;
use lumino_note_core::midi_types::TempoPoint;

impl EditorData {
    /// 重置编辑器数据到初始状态（释放所有内存）
    pub fn reset(&mut self) {
        self.edited_tracks.clear();
        self.onion_dirty_tracks = None;
        // 文档即将销毁：脏区必须丢弃，否则切回 CPU 时会被误当成真实变化。
        // 开关状态保留（后端选择不因 reset 而改变）。
        self.clear_cpu_onion_dirty();
        self.history_pending_changes.clear();
        self.history_pending_full = false;
        self.history_replay = false;
        self.mark_track_notes_changed();
        self.current_track = 0;
        self.history.clear();
        self.pending_commit = None;
        self.document = None;
        self.automation_lanes.clear();
        self.set_tempo_points(vec![TempoPoint {
            tick: 0.0,
            bpm: 120.0,
        }]);
        self.time_signatures = vec![(0, 4, 4)];
        self.arrange_selection.clear();
        // 主音轨增量对账：重置 = 全量重建（事件队列不可信）
        self.note_delta_events.clear();
        self.note_delta_dirty = true;
        self.track_visual_order.clear();
        // 重置 = 干净状态（无未保存更改）。注意：上方 `mark_track_notes_changed`
        // 与 `set_tempo_points` 会置位 `modified`，此处必须最后复位。
        self.modified = false;
    }
}
