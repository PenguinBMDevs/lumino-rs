//! 位置/跳转/同步

use super::core::PlaybackEngine;
use super::loop_cache::scan_pending_offs;

impl PlaybackEngine {
    /// 获取当前tick
    pub fn current_tick(&self) -> f32 {
        self.lock_playback()
            .map_or(0.0, |playback_state| playback_state.current_tick())
    }

    /// 跳转
    pub fn seek(&mut self, tick: f32) {
        self.seek_playback(tick);
        // 重设全部轨游标到 seek_tick 位置（PREF-006 A1：当前轨亦流式，
        // 无预建队列需要重建）
        self.reset_cursors_to(tick);
        // 模态状态追齐：把 seek 点之前的最后 CC/PC/PB/RPN/打击乐模态状态排队，
        // 由命令层 flush 到输出（暂停中 seek 也发，保证按 Play 时状态正确）。
        // `compute_chase` 同时返回该点的打击乐模态，用于同步引擎内部跟踪器
        // （否则 seek 后的 Bank Select 会与旧状态比较而漏切换）。
        let (chase_messages, chase_percussion) = self.compute_chase(tick);
        self.pending_chase = chase_messages;
        self.percussion = chase_percussion;
    }

    /// 将各音轨读取状态定位到指定 tick 位置
    pub(crate) fn reset_cursors_to(&mut self, tick: f32) {
        let Some(doc) = self.document.as_ref() else {
            return;
        };
        let seek_tick = tick as u32;
        for track_idx in 0..self.track_states.len() {
            // ChunkedList::partition_point(tick) = 第一个 tick >= seek_tick 的索引
            // （等价于旧 `notes.partition_point(|n| n.start_tick < seek_tick)`）
            let (cursor, pending_offs) = scan_pending_offs(doc.track_notes(track_idx), seek_tick);
            let state = &mut self.track_states[track_idx];
            state.note_cursor = cursor;
            state.pending_offs = pending_offs;
        }
        // 重置控制事件游标（ChunkedList 分块二分）
        self.control_event_cursor = doc.control_events.partition_point(seek_tick);
        // 重置额外 MIDI 事件游标
        self.midi_event_cursor = self
            .midi_events
            .partition_point(|event| event.tick < seek_tick as f32);
        self.last_processed_tick = tick;
    }
}
