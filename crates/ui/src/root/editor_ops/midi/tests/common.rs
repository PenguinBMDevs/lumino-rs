use crate::editor::note::Note;
use crate::root::Root;
use lumino_core::storage::config::UiConfig;
use std::sync::Arc;
use std::sync::atomic::AtomicU32;

/// 模拟 MIDI 输出连接，用于测试 playback 流程
pub struct MockOutput {
    note_on_count: Option<Arc<AtomicU32>>,
    note_off_count: Option<Arc<AtomicU32>>,
    cc_count: Option<Arc<AtomicU32>>,
    pb_count: Option<Arc<AtomicU32>>,
    /// 记录最后收到的 CC 参数
    pub last_cc: std::sync::Mutex<Option<(u8, u8, u8)>>,
    /// 记录最后收到的 PitchBend 参数
    pub last_pb: std::sync::Mutex<Option<(u8, f32)>>,
}

impl MockOutput {
    /// 创建一个不计数的 Mock 输出
    pub fn new() -> Self {
        Self {
            note_on_count: None,
            note_off_count: None,
            cc_count: None,
            pb_count: None,
            last_cc: std::sync::Mutex::new(None),
            last_pb: std::sync::Mutex::new(None),
        }
    }

    /// 创建带计数器的 Mock 输出
    pub fn with_counters(note_on_count: Arc<AtomicU32>, note_off_count: Arc<AtomicU32>) -> Self {
        Self {
            note_on_count: Some(note_on_count),
            note_off_count: Some(note_off_count),
            cc_count: None,
            pb_count: None,
            last_cc: std::sync::Mutex::new(None),
            last_pb: std::sync::Mutex::new(None),
        }
    }

    /// 创建带完整计数器的 Mock 输出（含 CC 和 PB）
    pub fn with_all_counters(
        note_on_count: Arc<AtomicU32>,
        note_off_count: Arc<AtomicU32>,
        cc_count: Arc<AtomicU32>,
        pb_count: Arc<AtomicU32>,
    ) -> Self {
        Self {
            note_on_count: Some(note_on_count),
            note_off_count: Some(note_off_count),
            cc_count: Some(cc_count),
            pb_count: Some(pb_count),
            last_cc: std::sync::Mutex::new(None),
            last_pb: std::sync::Mutex::new(None),
        }
    }
}

impl Default for MockOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl lumino_midi_io::OutputConnection for MockOutput {
    fn note_on(
        &mut self,
        _ch: u8,
        _key: u8,
        _vel: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        if let Some(counter) = &self.note_on_count {
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(())
    }
    fn note_off(
        &mut self,
        _ch: u8,
        _key: u8,
        _vel: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        if let Some(counter) = &self.note_off_count {
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        Ok(())
    }
    fn control_change(
        &mut self,
        ch: u8,
        controller: u8,
        value: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        if let Some(counter) = &self.cc_count {
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        if let Ok(mut last) = self.last_cc.lock() {
            *last = Some((ch, controller, value));
        }
        tracing::debug!(
            "MockOutput::control_change ch={} cc={} val={}",
            ch,
            controller,
            value
        );
        Ok(())
    }
    fn program_change(
        &mut self,
        _ch: u8,
        _program: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn pitch_bend(&mut self, ch: u8, value: f32) -> std::result::Result<(), lumino_midi_io::Error> {
        if let Some(counter) = &self.pb_count {
            counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        if let Ok(mut last) = self.last_pb.lock() {
            *last = Some((ch, value));
        }
        Ok(())
    }
    fn channel_pressure(
        &mut self,
        _ch: u8,
        _pressure: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn poly_pressure(
        &mut self,
        _ch: u8,
        _key: u8,
        _pressure: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn send_raw(&mut self, _data: [u8; 3]) -> std::result::Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn close(self: Box<Self>) {}
}

/// 辅助函数：创建带默认配置的 Root
pub fn create_root() -> Root {
    Root::new(&UiConfig::default())
}

/// 辅助函数：创建 MockOutput
pub fn create_mock_output() -> Box<MockOutput> {
    Box::new(MockOutput::new())
}

/// 添加两个测试音符
pub fn add_two_test_notes(root: &mut Root) {
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(480.0, 64, 480.0));
}
