//! 录制状态管理
//!
//! 管理 MIDI 录制过程中的状态，包括录制开关、前置音轨、节拍器等。

/// 录制状态
#[derive(Debug, Clone)]
pub struct RecordingState {
    /// 是否正在录制
    pub is_recording: bool,
    /// 输入设备名称
    pub input_device_name: Option<String>,
    /// 当前预备录制的音轨
    pub arm_track: usize,
    /// 节拍器开关
    pub metronome_enabled: bool,
    /// 录制中暂存的音符（key -> note_index），用于 NoteOff 时更新长度
    pub pending_notes: im::HashMap<u8, usize>,
}

impl Default for RecordingState {
    fn default() -> Self {
        Self::new()
    }
}

impl RecordingState {
    pub fn new() -> Self {
        Self {
            is_recording: false,
            input_device_name: None,
            arm_track: 0,
            metronome_enabled: false,
            pending_notes: im::HashMap::new(),
        }
    }

    /// 开始录制
    pub fn start(&mut self, device_name: Option<String>, track: usize) {
        self.is_recording = true;
        self.input_device_name = device_name;
        self.arm_track = track;
        self.pending_notes.clear();
    }

    /// 停止录制
    pub fn stop(&mut self) {
        self.is_recording = false;
        self.pending_notes.clear();
    }

    /// 切换节拍器
    pub fn toggle_metronome(&mut self) {
        self.metronome_enabled = !self.metronome_enabled;
    }
}
