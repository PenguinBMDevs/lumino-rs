//! Root 录制操作子模块

use crate::editor::note::Note;
use crate::editor::recording::RecordingState;
use crate::root::Root;

/// MIDI 通道掩码（提取状态字节的通道号）
const MIDI_CHANNEL_MASK: u8 = 0x0F;
/// MIDI 状态字节高位掩码
const MIDI_STATUS_MASK: u8 = 0xF0;
/// Note On 状态高位
const STATUS_NOTE_ON: u8 = 0x90;
/// Note Off 状态高位
const STATUS_NOTE_OFF: u8 = 0x80;
/// MIDI 数据值掩码（7 bit）
const MIDI_VALUE_MASK: u8 = 0x7F;

impl Root {
    /// 开始录制
    ///
    /// 打开第一个可用的 MIDI 输入设备，回调将原始数据写入共享缓冲区。
    pub fn start_recording(&mut self) {
        let api = match &self.midi_api {
            Some(api) => api,
            None => {
                tracing::warn!("录制: MIDI API 未初始化，无法录制");
                return;
            }
        };

        let inputs = match api.inputs() {
            Ok(inputs) => inputs,
            Err(e) => {
                tracing::error!("录制: 获取输入设备失败: {}", e);
                return;
            }
        };

        if inputs.is_empty() {
            tracing::warn!("录制: 没有可用的 MIDI 输入设备");
            return;
        }

        let device = &inputs[0];
        let device_name = device.name.clone();
        let buffer = self.midi_input_buffer.clone();

        let callback: lumino_midi::MidiInputCallback =
            Box::new(move |_timestamp: u64, data: &[u8]| {
                if let Ok(mut buf) = buffer.lock() {
                    buf.push_back(data.to_vec());
                }
            });

        match api.open_input(device.id, callback) {
            Ok(conn) => {
                let track = self.editor.editor_state.data.current_track;
                self.recording.start(Some(device_name), track);
                self.midi_input_connection = Some(conn);
                self.toolbar.is_recording = true;
                tracing::info!("录制: 已开始在设备上录制");
            }
            Err(e) => {
                tracing::error!("录制: 打开输入设备失败: {}", e);
            }
        }
    }

    /// 停止录制
    ///
    /// 关闭 MIDI 输入连接，处理残留的未关闭音符。
    pub fn stop_recording(&mut self) {
        if !self.recording.is_recording {
            return;
        }

        // 关闭 MIDI 输入连接
        if let Some(conn) = self.midi_input_connection.take() {
            conn.close();
        }

        // 处理残留的 pending 音符（设置一个默认长度）
        if !self.recording.pending_notes.is_empty() {
            let default_length = self.editor.editor_state.view.default_note_length.max(1.0);
            for (_, note_idx) in self.recording.pending_notes.iter() {
                if let Some(note) = self.editor.editor_state.data.notes.get_mut(*note_idx)
                    && note.length <= 0.0
                {
                    note.length = default_length;
                }
            }
            self.recording.pending_notes.clear();
            self.editor.mark_notes_changed();
        }

        // 推入 undo 历史
        if !self.editor.editor_state.data.notes.is_empty() {
            self.editor.push_history();
        }

        self.recording.stop();
        self.toolbar.is_recording = false;
        tracing::info!("录制: 已停止");
    }

    /// 轮询 MIDI 输入缓冲区并处理接收到的 MIDI 事件
    ///
    /// 在每帧更新时调用，将缓冲区中的原始 MIDI 数据转换为音符操作。
    pub fn poll_midi_input(&mut self) {
        if !self.recording.is_recording {
            return;
        }

        let mut notes_changed = false;
        let current_tick = self.editor.playback_position;

        let events: Vec<Vec<u8>> = {
            let mut buf = match self.midi_input_buffer.lock() {
                Ok(b) => b,
                Err(_) => return,
            };
            buf.drain(..).collect()
        };

        for data in &events {
            if data.is_empty() {
                continue;
            }

            let status = data[0];
            let msg_type = status & MIDI_STATUS_MASK;
            let _channel = status & MIDI_CHANNEL_MASK;

            if data.len() < 3 {
                continue;
            }

            let key = data[1] & MIDI_VALUE_MASK;
            let velocity = data[2] & MIDI_VALUE_MASK;

            match msg_type {
                STATUS_NOTE_ON if velocity > 0 => {
                    notes_changed |= self.handle_midi_note_on(key, velocity, current_tick);
                }
                STATUS_NOTE_OFF | STATUS_NOTE_ON => {
                    notes_changed |= self.handle_midi_note_off(key, current_tick);
                }
                _ => {}
            }
        }

        if notes_changed {
            self.editor.mark_notes_changed();
        }
    }

    /// 处理 MIDI NoteOn 事件：在当前位置创建音符
    fn handle_midi_note_on(&mut self, key: u8, velocity: u8, tick: f32) -> bool {
        // 避免同一按键重复 NoteOn（连奏时可能发生）
        if self.recording.pending_notes.contains_key(&key) {
            return false;
        }

        let note = Note::new(tick, key as u16, 0.0)
            .with_velocity(velocity)
            .with_channel(0);

        self.editor.editor_state.data.notes.push_back(note);
        let note_idx = self.editor.editor_state.data.notes.len() - 1;
        self.recording.pending_notes.insert(key, note_idx);

        tracing::debug!(
            "录制: NoteOn key={}, velocity={}, tick={:.2}, idx={}",
            key,
            velocity,
            tick,
            note_idx
        );
        true
    }

    /// 处理 MIDI NoteOff 事件：更新对应音符的长度
    fn handle_midi_note_off(&mut self, key: u8, tick: f32) -> bool {
        let note_idx = match self.recording.pending_notes.remove(&key) {
            Some(idx) => idx,
            None => return false,
        };

        if let Some(note) = self.editor.editor_state.data.notes.get_mut(note_idx) {
            let length = (tick - note.tick).max(1.0);
            note.length = length;

            tracing::debug!(
                "录制: NoteOff key={}, tick={:.2}, length={:.2}, idx={}",
                key,
                tick,
                length,
                note_idx
            );
            true
        } else {
            false
        }
    }

    /// 设置 MIDI API（供外部调用，如 MidiManager 初始化完成后）
    pub fn set_midi_api(&mut self, api: Box<dyn lumino_midi::Api>) {
        self.midi_api = Some(api);
        tracing::debug!("Root: MIDI API 已设置");
    }

    /// 获取录制状态引用
    pub fn recording_state(&self) -> &RecordingState {
        &self.recording
    }

    /// 获取录制状态可变引用
    pub fn recording_state_mut(&mut self) -> &mut RecordingState {
        &mut self.recording
    }
}
