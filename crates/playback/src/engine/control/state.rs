//! 状态管理（update 管线）

use super::super::{EventType, MidiMessage};
use super::core::PlaybackEngine;
use crate::PlaybackState;

impl PlaybackEngine {
    /// 获取播放状态
    pub fn state(&self) -> PlaybackState {
        self.lock_playback()
            .map_or(PlaybackState::Stopped, |p| p.state())
    }

    /// 处理播放更新
    ///
    /// 返回：需要发送的MIDI消息列表
    /// 当前音轨从 event_queue 读取，其他音轨从 document 流式读取
    pub fn update(&mut self) -> &mut Vec<MidiMessage> {
        self.reused_messages.clear();

        let (current_tick, is_playing) = {
            let Some(playback) = self.lock_playback() else {
                return &mut self.reused_messages;
            };
            (
                playback.current_tick(),
                playback.state() == PlaybackState::Playing,
            )
        };

        if !is_playing {
            self.last_processed_tick = current_tick;
            return &mut self.reused_messages;
        }

        // 临时取出 messages 避免 &mut self + &mut self.reused_messages 双重借用
        let mut messages = std::mem::take(&mut self.reused_messages);
        self.process_current_track(current_tick, &mut messages);
        self.process_other_tracks(current_tick, &mut messages);
        self.process_midi_events(current_tick, &mut messages);
        self.last_processed_tick = current_tick;
        self.handle_loop_wrap(current_tick, &mut messages);
        self.reused_messages = messages;

        &mut self.reused_messages
    }

    /// 处理当前音轨的事件队列
    fn process_current_track(&mut self, current_tick: f32, messages: &mut Vec<MidiMessage>) {
        while let Some(event) = self.event_queue.peek() {
            if event.tick > current_tick {
                break;
            }
            let event = if let Some(e) = self.event_queue.pop() {
                e
            } else {
                break;
            };
            Self::push_midi_message(event.event_type, messages);
        }
    }

    /// 处理其他音轨的事件（直接从 `MidiDocument` 音符切片流式读取）
    ///
    /// 每个非当前音轨维护一个 `note_cursor` 指向下一颗待触发 NoteOn 的音符，
    /// 并用最小堆保存已触发 NoteOn、等待 NoteOff 的音符。播放时按时间顺序
    /// 合并 NoteOn/NoteOff，避免预先把整轨事件拷贝排序。
    fn process_other_tracks(&mut self, current_tick: f32, messages: &mut Vec<MidiMessage>) {
        let Some(doc) = &self.document else { return };
        let tick_start_u = self.last_processed_tick as u32;
        let tick_end_u = current_tick as u32;

        for track_idx in 0..self.track_states.len() {
            if track_idx == self.current_track as usize {
                continue;
            }
            let notes = doc.track_notes(track_idx);
            if notes.is_empty() {
                continue;
            }
            let state = &mut self.track_states[track_idx];

            loop {
                let next_on_tick = notes
                    .get(state.note_cursor)
                    .map(|n| n.start_tick)
                    .unwrap_or(u32::MAX);
                let next_off_tick = state
                    .pending_offs
                    .peek()
                    .map(|off| off.end_tick)
                    .unwrap_or(u32::MAX);

                let next_tick = next_on_tick.min(next_off_tick);
                if next_tick > tick_end_u {
                    break;
                }

                if next_tick == next_on_tick {
                    let note = &notes[state.note_cursor];
                    if note.start_tick >= tick_start_u
                        && note.velocity > self.velocity_filter_threshold
                    {
                        messages.push(MidiMessage::NoteOn {
                            channel: note.channel,
                            key: note.key,
                            velocity: note.velocity,
                        });
                        state.pending_offs.push(super::core::PendingNoteOff {
                            end_tick: note.end_tick,
                            note_index: state.note_cursor,
                        });
                    }
                    state.note_cursor += 1;
                } else {
                    let Some(off) = state.pending_offs.pop() else {
                        break;
                    };
                    if off.end_tick >= tick_start_u {
                        let note = &notes[off.note_index];
                        messages.push(MidiMessage::NoteOff {
                            channel: note.channel,
                            key: note.key,
                        });
                    }
                }
            }
        }

        // ── 控制事件（CC/PC/PB）从 document 流式读取 ──
        //
        // 仅处理非当前音轨的控制事件。当前音轨的控制事件由 `process_midi_events`
        // 从 automation_lanes / midi_events 路径处理（支持实时编辑）。
        // 这样避免与 process_midi_events 重复发射同一事件（double-firing），
        // 同时确保编辑后的 CC 值不会被 doc.control_events 中的原始值覆盖。
        let ctrl_events = &doc.control_events;
        let ctrl_cursor = &mut self.control_event_cursor;
        while *ctrl_cursor < ctrl_events.len() {
            let ev = &ctrl_events[*ctrl_cursor];
            let ev_tick = ev.tick as f32;
            if ev_tick > current_tick {
                break;
            }
            if ev_tick >= self.last_processed_tick && ev.track != self.current_track {
                if ev.kind == 0 {
                    // 复制 packed 字段到局部变量，避免未对齐引用
                    let cc_tick = ev.tick;
                    let cc_track = ev.track;
                    let cc_ch = ev.channel;
                    let cc_param = ev.param;
                    tracing::debug!(
                        "process_other_tracks: CC 事件 (其他音轨) tick={} track={} ch={} param={}",
                        cc_tick,
                        cc_track,
                        cc_ch,
                        cc_param,
                    );
                }
                Self::push_control_event(ev, messages);
            }
            *ctrl_cursor += 1;
        }
    }

    /// 处理额外 MIDI 控制事件
    ///
    /// 使用游标推进，避免每次 update 线性扫描全部事件。
    /// 假设 midi_events 已按 tick 排序。
    fn process_midi_events(&mut self, current_tick: f32, messages: &mut Vec<MidiMessage>) {
        while self.midi_event_cursor < self.midi_events.len() {
            let ev = &self.midi_events[self.midi_event_cursor];
            if ev.tick > current_tick {
                break;
            }
            if ev.tick >= self.last_processed_tick {
                if matches!(ev.message, MidiMessage::ControlChange { .. }) {
                    tracing::debug!(
                        "process_midi_events: CC 事件触发 tick={} cursor={} cc={:?}",
                        ev.tick,
                        self.midi_event_cursor,
                        ev.message,
                    );
                }
                Self::push_midi_message_from_event(&ev.message, messages);
            }
            self.midi_event_cursor += 1;
        }
    }

    /// 处理循环回绕
    fn handle_loop_wrap(&mut self, current_tick: f32, _messages: &mut Vec<MidiMessage>) {
        if self.looping
            && let Some((_loop_start, loop_end)) = self.loop_range
            && current_tick >= loop_end
        {
            let loop_start = self.loop_range.map_or(0.0, |(s, _)| s);
            if let Some(mut playback) = self.lock_playback() {
                playback.seek(loop_start);
            }
            let seek_tick_u = loop_start as u32;
            if let Some(doc) = self.document.clone() {
                for track_idx in 0..self.track_states.len() {
                    if track_idx == self.current_track as usize {
                        continue;
                    }
                    self.reset_track_state_to(track_idx, seek_tick_u, &doc);
                }
                let ctrl_events = &doc.control_events;
                self.control_event_cursor = ctrl_events.partition_point(|ev| ev.tick < seek_tick_u);
                self.midi_event_cursor =
                    self.midi_events.partition_point(|ev| ev.tick < loop_start);
            }
            self.rebuild_queue_from_current_track(Some(loop_start));
            self.last_processed_tick = loop_start;
        }
    }

    #[inline]
    fn push_control_event(ev: &midly::loader::PackedControlEvent, messages: &mut Vec<MidiMessage>) {
        match ev.kind {
            0 => {
                let (controller, value) = ev.as_control_change();
                messages.push(MidiMessage::ControlChange {
                    channel: ev.channel,
                    controller,
                    value,
                });
            }
            1 => {
                let program = ev.as_program_change();
                messages.push(MidiMessage::ProgramChange {
                    channel: ev.channel,
                    program,
                });
            }
            2 => {
                let value = ev.as_pitch_bend();
                messages.push(MidiMessage::PitchBend {
                    channel: ev.channel,
                    value,
                });
            }
            _ => {}
        }
    }

    #[inline]
    fn push_midi_message(event_type: EventType, messages: &mut Vec<MidiMessage>) {
        match event_type {
            EventType::NoteOn {
                channel,
                key,
                velocity,
            } => {
                messages.push(MidiMessage::NoteOn {
                    channel,
                    key,
                    velocity,
                });
            }
            EventType::NoteOff { channel, key } => {
                messages.push(MidiMessage::NoteOff { channel, key });
            }
        }
    }

    #[inline]
    fn push_midi_message_from_event(msg: &MidiMessage, messages: &mut Vec<MidiMessage>) {
        messages.push(msg.clone());
    }
}
