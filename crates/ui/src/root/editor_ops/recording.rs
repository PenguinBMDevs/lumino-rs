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
    /// 打开选中的 MIDI 输入设备（或第一个可用设备），回调将原始数据写入共享缓冲区。
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

        // 使用设置面板选中的设备，或回退到第一个设备
        let device = self
            .settings
            .selected_midi_device
            .and_then(|id| inputs.iter().find(|d| d.id == id))
            .unwrap_or(&inputs[0]);

        // 同步选中设备到设置面板
        self.settings.selected_midi_device = Some(device.id);

        let device_id = device.id;
        let device_name = device.name.clone();
        let buffer = self.midi_input_buffer.clone();

        let callback: lumino_midi::MidiInputCallback =
            Box::new(move |_timestamp: u64, data: &[u8]| {
                if let Ok(mut buf) = buffer.lock() {
                    buf.push_back(data.to_vec());
                }
            });

        match api.open_input(device_id, callback) {
            Ok(conn) => {
                let track = self.editor.editor_state.data.current_track;
                self.recording.start(Some(device_name.clone()), track);
                self.midi_input_connection = Some(conn);
                self.toolbar.is_recording = true;
                tracing::info!(
                    "录制: 已开始在设备 \"{}\" (#{}) 上录制",
                    device_name,
                    device_id
                );
            }
            Err(e) => {
                tracing::error!("录制: 打开输入设备 #{} 失败: {}", device_id, e);
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
    /// 使用墙钟时间计算当前 tick，使演奏指示线在录制时匀速滚动。
    pub fn poll_midi_input(&mut self) {
        if !self.recording.is_recording {
            return;
        }

        // 从录制开始时间计算当前 tick（默认 120 BPM = 500000 µs/拍）
        let ppq = self.editor.editor_state.view.ppq as f64;
        let default_tempo_micros_per_beat: f64 = 500_000.0;
        let elapsed_micros = self
            .recording
            .started_at
            .map(|t| t.elapsed().as_micros() as f64)
            .unwrap_or(0.0);
        let current_tick = (elapsed_micros * ppq / default_tempo_micros_per_beat) as f32;
        self.editor.playback_position = current_tick;

        let mut notes_changed = false;

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

        let note = Note::from_raw(tick, key as u16, 0.0, velocity, 0);

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
        // 缓存设备列表到设置面板（供设备选择器使用）
        let devices = api.inputs().unwrap_or_default();
        self.settings.midi_devices = devices.iter().map(|d| (d.id, d.name.clone())).collect();

        // 自动选中第一个设备（如果还没有选中）
        if self.settings.selected_midi_device.is_none()
            && let Some(first) = devices.first()
        {
            self.settings.selected_midi_device = Some(first.id);
        }

        self.midi_api = Some(api);
        tracing::debug!("Root: MIDI API 已设置，检测到 {} 个输入设备", devices.len());
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
