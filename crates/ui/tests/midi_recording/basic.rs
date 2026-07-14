//! MIDI 录制基础功能测试
//!
//! 测试覆盖：
//! - 基本的 NoteOn → NoteOff 生命周期
//! - 和弦录制（多音符同时录制）
//! - 重复 NoteOn 防护
//! - 录制不干扰现有音符

use lumino_core::storage::config::UiConfig;
use lumino_ui::editor::note::Note;
use lumino_ui::message::Message;
use lumino_ui::root::Root;
use lumino_ui::toolbar;
use std::sync::{Arc, Mutex};

// ════════════════════════════════════════════════════════════════════════════
// Mock MIDI API 基础设施
// ════════════════════════════════════════════════════════════════════════════

/// 模拟 MIDI 输入连接，允许测试代码直接注入 MIDI 字节
struct MockInputConnection {
    _device_id: u32,
}

impl lumino_midi_io::InputConnection for MockInputConnection {
    fn close(self: Box<Self>) {
        tracing::debug!("MockInputConnection: 关闭设备 #{}", self._device_id);
    }
}

/// 模拟 MIDI API，返回固定设备列表，open_input 时捕获回调
pub(super) struct MockMidiApi {
    inputs: Vec<lumino_midi_io::InputInfo>,
    // 存储 open_input 时收到的回调，测试代码可通过此回调注入 MIDI 数据
    callback: Arc<Mutex<Option<lumino_midi_io::MidiInputCallback>>>,
}

impl MockMidiApi {
    pub(super) fn new() -> Self {
        Self {
            inputs: vec![
                lumino_midi_io::InputInfo {
                    id: 0,
                    name: "Mock MIDI Keyboard".to_string(),
                },
                lumino_midi_io::InputInfo {
                    id: 1,
                    name: "Mock MIDI Pad".to_string(),
                },
            ],
            callback: Arc::new(Mutex::new(None)),
        }
    }

    /// 注入 MIDI 字节到回调（模拟硬件输入）
    #[expect(dead_code)]
    pub(super) fn inject_midi(&self, data: &[u8]) {
        if let Ok(mut cb_opt) = self.callback.lock() {
            if let Some(cb) = cb_opt.as_mut() {
                cb(0, data);
            }
        }
    }
}

impl lumino_midi_io::Api for MockMidiApi {
    fn version(&self) -> Option<String> {
        Some("mock-1.0".to_string())
    }

    fn inputs(&self) -> Result<Vec<lumino_midi_io::InputInfo>, lumino_midi_io::Error> {
        Ok(self.inputs.clone())
    }

    fn outputs(&self) -> Result<Vec<lumino_midi_io::OutputInfo>, lumino_midi_io::Error> {
        Ok(vec![])
    }

    fn open_output(
        &self,
        _id: u32,
    ) -> Result<Box<dyn lumino_midi_io::OutputConnection>, lumino_midi_io::Error> {
        Err(lumino_midi_io::Error::OpenOutputFailed(
            "mock 无输出".to_string(),
        ))
    }

    fn open_input(
        &self,
        id: u32,
        callback: lumino_midi_io::MidiInputCallback,
    ) -> Result<Box<dyn lumino_midi_io::InputConnection>, lumino_midi_io::Error> {
        if let Ok(mut cb_opt) = self.callback.lock() {
            *cb_opt = Some(callback);
        }
        Ok(Box::new(MockInputConnection { _device_id: id }))
    }
}

// ════════════════════════════════════════════════════════════════════════════
// 测试用例
// ════════════════════════════════════════════════════════════════════════════

/// 测试 1：最基本的 NoteOn → NoteOff 录制
///
/// 验证：
/// - 开始录制后状态正确
/// - 一个 NoteOn 创建一个音符（length=0）
/// - 对应的 NoteOff 更新音符长度
/// - 停止录制后音符保留在编辑器中
/// - undo 历史被正确推送
#[test]
fn test_single_note_recording_lifecycle() {
    let mut root = Root::new(&UiConfig::default());

    // 手动注入 API（测试专用路径）
    root.midi.api = Some(Box::new(MockMidiApi::new()));

    // 开始录制
    root.update(Message::Toolbar(toolbar::Event::Record));
    assert!(root.recording.is_recording, "录制应已启动");
    assert!(root.toolbar.is_recording, "工具栏录制标志应为 true");

    // 模拟 MIDI 输入：C4 NoteOn (key=60, velocity=100)
    // 注意：由于我们无法直接访问回调，需要通过模拟的方式
    // 这里直接操作 midi_input_buffer（测试中允许）
    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("锁定MIDI输入缓冲区失败");
        buf.push_back(vec![0x90, 60, 100]); // NoteOn C4 vel=100
    }

    // 轮询处理
    root.poll_midi_input();

    // 验证音符已创建
    assert_eq!(root.editor.editor_state.data.notes.len(), 1, "应有一个音符");
    let note = &root.editor.editor_state.data.notes[0];
    assert_eq!(note.key, 60, "音符键应为 C4 (60)");
    assert_eq!(note.velocity, 100, "音符力度应为 100");
    assert_eq!(note.length, 0.0, "NoteOn 后长度应为 0（等待 NoteOff）");

    // 模拟时间流逝后 NoteOff
    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("锁定MIDI输入缓冲区失败");
        buf.push_back(vec![0x80, 60, 0]); // NoteOff C4
    }

    root.poll_midi_input();

    // 验证音符长度已更新
    let note = &root.editor.editor_state.data.notes[0];
    assert!(note.length > 0.0, "NoteOff 后长度应大于 0");

    // 停止录制
    root.update(Message::Toolbar(toolbar::Event::RecordStop));
    assert!(!root.recording.is_recording, "录制应已停止");
    assert!(!root.toolbar.is_recording, "工具栏录制标志应为 false");

    // 验证音符保留在编辑器中
    assert_eq!(
        root.editor.editor_state.data.notes.len(),
        1,
        "停止后音符应保留"
    );

    // 验证 undo 历史
    assert!(root.editor.can_undo(), "录制后应有 undo 历史");
}

/// 测试 2：多音符同时录制（和弦）
///
/// 验证：
/// - 多个 NoteOn 创建多个音符
/// - 各自的 NoteOff 只影响对应音符
/// - 不会混淆不同键的音符
#[test]
fn test_chord_recording() {
    let mut root = Root::new(&UiConfig::default());
    root.midi.api = Some(Box::new(MockMidiApi::new()));

    // 开始录制
    root.update(Message::Toolbar(toolbar::Event::Record));

    // C大调和弦：C4 + E4 + G4 同时按下
    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("锁定MIDI输入缓冲区失败");
        buf.push_back(vec![0x90, 60, 100]); // C4
        buf.push_back(vec![0x90, 64, 100]); // E4
        buf.push_back(vec![0x90, 67, 100]); // G4
    }
    root.poll_midi_input();

    assert_eq!(root.editor.editor_state.data.notes.len(), 3, "应有三个音符");

    // 分别释放（顺序不重要）
    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("锁定MIDI输入缓冲区失败");
        buf.push_back(vec![0x80, 64, 0]); // E4 off
        buf.push_back(vec![0x80, 67, 0]); // G4 off
        buf.push_back(vec![0x80, 60, 0]); // C4 off
    }
    root.poll_midi_input();

    // 验证三个音符都有正长度
    for (i, note) in root.editor.editor_state.data.notes.iter().enumerate() {
        assert!(
            note.length > 0.0,
            "音符 {} (key={}) 应有正长度",
            i,
            note.key
        );
    }

    // 验证 pending_notes 已清空（所有 NoteOff 已处理）
    assert!(
        root.recording.pending_notes.is_empty(),
        "所有 NoteOff 处理后 pending_notes 应为空"
    );

    root.update(Message::Toolbar(toolbar::Event::RecordStop));
}

/// 测试 3：重复 NoteOn 防护
///
/// 验证：同一键在未收到 NoteOff 前再次收到 NoteOn 被忽略
#[test]
fn test_duplicate_note_on_ignored() {
    let mut root = Root::new(&UiConfig::default());
    root.midi.api = Some(Box::new(MockMidiApi::new()));

    root.update(Message::Toolbar(toolbar::Event::Record));

    // 同一个键连续两次 NoteOn
    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("锁定MIDI输入缓冲区失败");
        buf.push_back(vec![0x90, 60, 100]); // NoteOn C4
        buf.push_back(vec![0x90, 60, 80]); // 重复 NoteOn C4（应被忽略）
    }
    root.poll_midi_input();

    assert_eq!(
        root.editor.editor_state.data.notes.len(),
        1,
        "重复 NoteOn 应只创建一个音符"
    );

    // NoteOff
    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("锁定MIDI输入缓冲区失败");
        buf.push_back(vec![0x80, 60, 0]);
    }
    root.poll_midi_input();

    let note = &root.editor.editor_state.data.notes[0];
    assert_eq!(note.velocity, 100, "应保留第一个 NoteOn 的力度");

    root.update(Message::Toolbar(toolbar::Event::RecordStop));
}

/// 测试 10：录制不干扰现有音符
///
/// 验证：录制新音符不会删除编辑器中已有的音符
#[test]
fn test_recording_preserves_existing_notes() {
    let mut root = Root::new(&UiConfig::default());
    root.midi.api = Some(Box::new(MockMidiApi::new()));

    // 先添加一些现有音符
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 48, 480.0).with_velocity(80));
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(480.0, 52, 480.0).with_velocity(80));

    root.update(Message::Toolbar(toolbar::Event::Record));

    // 录制新音符
    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("锁定MIDI输入缓冲区失败");
        buf.push_back(vec![0x90, 60, 100]);
        buf.push_back(vec![0x80, 60, 0]);
    }
    root.poll_midi_input();

    assert_eq!(
        root.editor.editor_state.data.notes.len(),
        3,
        "应有 3 个音符（2 个原有 + 1 个新录制）"
    );

    // 验证原有音符未受影响
    assert_eq!(root.editor.editor_state.data.notes[0].key, 48);
    assert_eq!(root.editor.editor_state.data.notes[1].key, 52);

    root.update(Message::Toolbar(toolbar::Event::RecordStop));
}
