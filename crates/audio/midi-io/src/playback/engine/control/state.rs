//! 状态管理（update 管线）

use super::super::{EventType, MidiMessage};
use super::core::PlaybackEngine;
use crate::playback::PlaybackState;

impl PlaybackEngine {
    /// 获取播放状态
    pub fn state(&self) -> PlaybackState {
        self.lock_playback()
            .map_or(PlaybackState::Stopped, |playback| playback.state())
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

        // 轨尾标自动停止：未开启循环且播放头越过（时间）最靠后音符的结束 tick 时，
        // 播放停止在轨尾标处。与编辑/走带视图共用同一权威值 `tracks_max_end_tick()`，
        // 编辑期间在最后音符后追加音符会扩展该值，播放停止点随之后移。
        // 注意：循环播放（looping）由 `handle_loop_wrap` 处理回绕，此处不介入。
        if !self.looping {
            let end_tick = self
                .document
                .as_ref()
                .map(|doc| doc.tracks_max_end_tick())
                .unwrap_or(0);
            if end_tick > 0 && current_tick >= end_tick as f32 {
                if let Some(mut playback) = self.lock_playback() {
                    playback.stop();
                }
                self.last_processed_tick = 0.0;
            }
        }

        &mut self.reused_messages
    }

    /// 处理当前音轨的事件队列
    fn process_current_track(&mut self, current_tick: f32, messages: &mut Vec<MidiMessage>) {
        while let Some(event) = self.event_queue.peek() {
            if event.tick > current_tick {
                break;
            }
            let event = if let Some(popped_event) = self.event_queue.pop() {
                popped_event
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
            // 静音/独奏过滤：被静音或未被独奏的音轨不发声。
            if !self.track_should_play(track_idx) {
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
                    .map(|note| note.start_tick)
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
        while self.control_event_cursor < ctrl_events.len() {
            let ctrl_event = &ctrl_events[self.control_event_cursor];
            let ctrl_tick = ctrl_event.tick as f32;
            if ctrl_tick > current_tick {
                break;
            }
            if ctrl_tick >= self.last_processed_tick && ctrl_event.track != self.current_track {
                // 静音/独奏过滤：被静音或未被独奏的音轨其控制事件也不发送。
                if self.track_should_play(ctrl_event.track as usize) {
                    if ctrl_event.kind == 0 {
                        // 复制 packed 字段到局部变量，避免未对齐引用
                        let cc_tick = ctrl_event.tick;
                        let cc_track = ctrl_event.track;
                        let cc_ch = ctrl_event.channel;
                        let cc_param = ctrl_event.param;
                        tracing::debug!(
                            "process_other_tracks: CC 事件 (其他音轨) tick={} track={} ch={} param={}",
                            cc_tick,
                            cc_track,
                            cc_ch,
                            cc_param,
                        );
                    }
                    Self::push_control_event(ctrl_event, messages);
                }
            }
            self.control_event_cursor += 1;
        }
    }

    /// 处理额外 MIDI 控制事件
    ///
    /// 使用游标推进，避免每次 update 线性扫描全部事件。
    /// 假设 midi_events 已按 tick 排序。
    fn process_midi_events(&mut self, current_tick: f32, messages: &mut Vec<MidiMessage>) {
        while self.midi_event_cursor < self.midi_events.len() {
            let midi_event = &self.midi_events[self.midi_event_cursor];
            if midi_event.tick > current_tick {
                break;
            }
            if midi_event.tick >= self.last_processed_tick {
                if matches!(midi_event.message, MidiMessage::ControlChange { .. }) {
                    tracing::debug!(
                        "process_midi_events: CC 事件触发 tick={} cursor={} cc={:?}",
                        midi_event.tick,
                        self.midi_event_cursor,
                        midi_event.message,
                    );
                }
                Self::push_midi_message_from_event(&midi_event.message, messages);
            }
            self.midi_event_cursor += 1;
        }
    }

    /// 处理循环回绕
    fn handle_loop_wrap(&mut self, current_tick: f32, messages: &mut Vec<MidiMessage>) {
        if self.looping
            && let Some((_loop_start, loop_end)) = self.loop_range
            && current_tick >= loop_end
        {
            let loop_start = self.loop_range.map_or(0.0, |(start, _)| start);
            if let Some(mut playback) = self.lock_playback() {
                playback.seek(loop_start);
            }
            let seek_tick_u = loop_start as u32;
            if let Some(doc) = self.document.as_ref() {
                for track_idx in 0..self.track_states.len() {
                    if track_idx == self.current_track as usize {
                        continue;
                    }
                    let notes = doc.track_notes(track_idx);
                    let state = &mut self.track_states[track_idx];
                    state.pending_offs.clear();
                    state.note_cursor = notes.partition_point(seek_tick_u);
                    for (note_idx, note) in notes.iter().enumerate().take(state.note_cursor) {
                        if note.end_tick >= seek_tick_u {
                            state.pending_offs.push(super::core::PendingNoteOff {
                                end_tick: note.end_tick,
                                note_index: note_idx,
                            });
                        }
                    }
                }
                let ctrl_events = &doc.control_events;
                self.control_event_cursor = ctrl_events.partition_point(seek_tick_u);
                self.midi_event_cursor = self
                    .midi_events
                    .partition_point(|midi_event| midi_event.tick < loop_start);
            }
            self.rebuild_queue_from_current_track(Some(loop_start));
            self.last_processed_tick = loop_start;
            // 循环回绕同样是 seek：chase 重放回绕点之前的控制器状态，
            // 否则第二轮循环音色/弯音/踏板停留在循环尾状态。
            messages.extend(self.compute_chase_messages());
        }
    }

    #[inline]
    fn push_control_event(
        event: &midly::loader::PackedControlEvent,
        messages: &mut Vec<MidiMessage>,
    ) {
        match event.kind {
            0 => {
                let (controller, value) = event.as_control_change();
                messages.push(MidiMessage::ControlChange {
                    channel: event.channel,
                    controller,
                    value,
                });
            }
            1 => {
                let program = event.as_program_change();
                messages.push(MidiMessage::ProgramChange {
                    channel: event.channel,
                    program,
                });
            }
            2 => {
                let value = event.as_pitch_bend();
                messages.push(MidiMessage::PitchBend {
                    channel: event.channel,
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
