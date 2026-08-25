//! 位置/跳转/同步

use super::chase::ChannelChase;
use super::core::{PendingNoteOff, PlaybackEngine};
use crate::playback::engine::MidiMessage;

impl PlaybackEngine {
    /// 获取当前tick
    pub fn current_tick(&self) -> f32 {
        self.lock_playback()
            .map_or(0.0, |playback_state| playback_state.current_tick())
    }

    /// 跳转
    ///
    /// 返回 chase 重放消息（seek 点之前生效的 CC/PC/PB/RPN 状态），
    /// 由调用方在 `all_notes_off + reset_control` 清理之后发送到输出连接，
    /// 保证 seek 后音色/弯音/踏板与跳转点一致。
    pub fn seek(&mut self, tick: f32) -> Vec<MidiMessage> {
        self.seek_playback(tick);
        // 重设游标到 seek_tick 位置
        self.reset_cursors_to(tick);
        // 重建当前轨事件队列
        self.rebuild_queue_from_current_track(Some(tick));
        // 收集跳转点之前的控制器状态快照并生成重放消息
        self.compute_chase_messages()
    }

    /// 计算 chase 重放消息：扫描 seek 点之前的全部控制事件
    /// （document 其他轨 [0, control_event_cursor) + 当前轨可编辑事件
    /// [0, midi_event_cursor)），按通道做 latest-wins 快照后输出。
    ///
    /// 静音/未独奏的音轨不参与 chase（与播放过滤规则一致，避免复活被静音轨道的控制器）。
    pub(crate) fn compute_chase_messages(&self) -> Vec<MidiMessage> {
        let Some(doc) = self.document.as_ref() else {
            return Vec::new();
        };
        let mut states: Vec<ChannelChase> = (0..16).map(|_| ChannelChase::default()).collect();

        // 其他轨的控制事件（当前轨的 CC 由 midi_events 可编辑路径处理，见
        // process_other_tracks 的 double-firing 说明——chase 同样只取一侧）
        let cursor = self.control_event_cursor.min(doc.control_events.len());
        for i in 0..cursor {
            let ev = match doc.control_events.get(i) {
                Some(ev) => ev,
                None => break,
            };
            if ev.track != self.current_track && self.track_should_play(ev.track as usize) {
                states[(ev.channel & 0x0F) as usize].apply(ev.kind, ev.param);
            }
        }

        // 当前轨的可编辑控制事件（automation lanes / midi_events）
        if self.track_should_play(self.current_track as usize)
            && let Some(current_state) = states.get_mut(self.current_track_channel_index())
        {
            let midi_cursor = self.midi_event_cursor.min(self.midi_events.len());
            for event in &self.midi_events[..midi_cursor] {
                current_state.apply_message(&event.message);
            }
        }

        let mut messages = Vec::new();
        for (channel, state) in states.iter().enumerate() {
            messages.extend(state.emit(channel as u8));
        }
        messages
    }

    /// 当前轨在通道状态数组中的索引（低 4 位通道号；无文档时为 0）
    fn current_track_channel_index(&self) -> usize {
        self.document.as_ref().map_or(0, |doc| {
            (doc.track_channel(self.current_track) & 0x0F) as usize
        })
    }

    /// 将各音轨读取状态定位到指定 tick 位置
    pub(crate) fn reset_cursors_to(&mut self, tick: f32) {
        let Some(doc) = self.document.as_ref() else {
            return;
        };
        let seek_tick = tick as u32;
        for track_idx in 0..self.track_states.len() {
            if track_idx == self.current_track as usize {
                continue;
            }
            let notes = doc.track_notes(track_idx);
            let state = &mut self.track_states[track_idx];
            state.pending_offs.clear();
            // ChunkedList::partition_point(tick) = 第一个 tick >= seek_tick 的索引
            // （等价于旧 `notes.partition_point(|n| n.start_tick < seek_tick)`）
            state.note_cursor = notes.partition_point(seek_tick);
            for (note_idx, note) in notes.iter().enumerate().take(state.note_cursor) {
                if note.end_tick >= seek_tick {
                    state.pending_offs.push(PendingNoteOff {
                        end_tick: note.end_tick,
                        note_index: note_idx,
                    });
                }
            }
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
