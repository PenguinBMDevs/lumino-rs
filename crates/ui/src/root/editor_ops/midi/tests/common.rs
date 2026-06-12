use crate::editor::note::Note;
use crate::root::Root;
use lumino_core::storage::config::UiConfig;
use std::sync::Arc;
use std::sync::atomic::AtomicU32;

/// 模拟 MIDI 输出连接，用于测试 playback 流程
pub struct MockOutput {
    note_on_count: Option<Arc<AtomicU32>>,
    note_off_count: Option<Arc<AtomicU32>>,
}

impl MockOutput {
    /// 创建一个不计数的 Mock 输出
    pub fn new() -> Self {
        Self {
            note_on_count: None,
            note_off_count: None,
        }
    }

    /// 创建带计数器的 Mock 输出
    pub fn with_counters(note_on_count: Arc<AtomicU32>, note_off_count: Arc<AtomicU32>) -> Self {
        Self {
            note_on_count: Some(note_on_count),
            note_off_count: Some(note_off_count),
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
        _ch: u8,
        _controller: u8,
        _value: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn program_change(
        &mut self,
        _ch: u8,
        _program: u8,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
        Ok(())
    }
    fn pitch_bend(
        &mut self,
        _ch: u8,
        _value: f32,
    ) -> std::result::Result<(), lumino_midi_io::Error> {
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
