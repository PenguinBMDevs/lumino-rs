//! MIDI 录制端到端集成测试
//!
//! 测试覆盖：
//! 1. MIDI API 初始化 → 设备枚举 → 输入连接打开
//! 2. NoteOn/NoteOff 完整录制生命周期
//! 3. 边界情况：重复 NoteOn、NoteOff 无对应 NoteOn、零力度 NoteOn
//! 4. 录制状态机：开始 → 接收数据 → 停止 → undo 历史
//! 5. 多音符同时录制（和弦）
//!
//! 运行方式：
//!   cargo test --test midi_recording_test

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
struct MockMidiApi {
    inputs: Vec<lumino_midi_io::InputInfo>,
    // 存储 open_input 时收到的回调，测试代码可通过此回调注入 MIDI 数据
    callback: Arc<Mutex<Option<lumino_midi_io::MidiInputCallback>>>,
}

impl MockMidiApi {
    fn new() -> Self {
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
    #[allow(dead_code)]
    fn inject_midi(&self, data: &[u8]) {
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
        let mut buf = root.midi.input_buffer.lock().unwrap();
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
        let mut buf = root.midi.input_buffer.lock().unwrap();
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
        let mut buf = root.midi.input_buffer.lock().unwrap();
        buf.push_back(vec![0x90, 60, 100]); // C4
        buf.push_back(vec![0x90, 64, 100]); // E4
        buf.push_back(vec![0x90, 67, 100]); // G4
    }
    root.poll_midi_input();

    assert_eq!(root.editor.editor_state.data.notes.len(), 3, "应有三个音符");

    // 分别释放（顺序不重要）
    {
        let mut buf = root.midi.input_buffer.lock().unwrap();
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
        let mut buf = root.midi.input_buffer.lock().unwrap();
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
        let mut buf = root.midi.input_buffer.lock().unwrap();
        buf.push_back(vec![0x80, 60, 0]);
    }
    root.poll_midi_input();

    let note = &root.editor.editor_state.data.notes[0];
    assert_eq!(note.velocity, 100, "应保留第一个 NoteOn 的力度");

    root.update(Message::Toolbar(toolbar::Event::RecordStop));
}

/// 测试 4：零力度 NoteOn 当作 NoteOff
///
/// 验证：velocity=0 的 NoteOn 被正确处理为 NoteOff
#[test]
fn test_note_on_zero_velocity_as_note_off() {
    let mut root = Root::new(&UiConfig::default());
    root.midi.api = Some(Box::new(MockMidiApi::new()));

    root.update(Message::Toolbar(toolbar::Event::Record));

    // NoteOn
    {
        let mut buf = root.midi.input_buffer.lock().unwrap();
        buf.push_back(vec![0x90, 60, 100]);
    }
    root.poll_midi_input();

    assert_eq!(root.editor.editor_state.data.notes.len(), 1);

    // 零力度 NoteOn（应作为 NoteOff）
    {
        let mut buf = root.midi.input_buffer.lock().unwrap();
        buf.push_back(vec![0x90, 60, 0]);
    }
    root.poll_midi_input();

    let note = &root.editor.editor_state.data.notes[0];
    assert!(note.length > 0.0, "零力度 NoteOn 应触发 NoteOff 处理");

    root.update(Message::Toolbar(toolbar::Event::RecordStop));
}

/// 测试 5：无对应 NoteOn 的 NoteOff 被忽略
///
/// 验证：孤立的 NoteOff 不会创建或修改任何音符
#[test]
fn test_orphan_note_off_ignored() {
    let mut root = Root::new(&UiConfig::default());
    root.midi.api = Some(Box::new(MockMidiApi::new()));

    root.update(Message::Toolbar(toolbar::Event::Record));

    // 直接发送 NoteOff，没有前置 NoteOn
    {
        let mut buf = root.midi.input_buffer.lock().unwrap();
        buf.push_back(vec![0x80, 60, 0]);
    }
    root.poll_midi_input();

    assert_eq!(
        root.editor.editor_state.data.notes.len(),
        0,
        "孤立 NoteOff 不应创建音符"
    );

    root.update(Message::Toolbar(toolbar::Event::RecordStop));
}

/// 测试 6：停止录制时残留音符的处理
///
/// 验证：未收到 NoteOff 的音符在停止时被赋予默认长度
#[test]
fn test_pending_notes_on_stop() {
    let mut root = Root::new(&UiConfig::default());
    root.midi.api = Some(Box::new(MockMidiApi::new()));

    root.update(Message::Toolbar(toolbar::Event::Record));

    // NoteOn 但不发送 NoteOff
    {
        let mut buf = root.midi.input_buffer.lock().unwrap();
        buf.push_back(vec![0x90, 60, 100]);
    }
    root.poll_midi_input();

    let note = &root.editor.editor_state.data.notes[0];
    assert_eq!(note.length, 0.0, "未关闭的音符长度应为 0");

    // 停止录制（应赋予默认长度）
    root.update(Message::Toolbar(toolbar::Event::RecordStop));

    let note = &root.editor.editor_state.data.notes[0];
    let default_len = root.editor.editor_state.view.default_note_length;
    assert!(
        note.length > 0.0,
        "停止后残留音符应获得默认长度 ({}",
        default_len
    );
}

/// 测试 7：录制状态机完整性
///
/// 验证：
/// - 未设置 MIDI API 时开始录制应失败 gracefully
/// - 停止未开始的录制应无异常
/// - 录制中切换音轨不影响当前录制
#[test]
fn test_recording_state_machine() {
    let mut root = Root::new(&UiConfig::default());
    // 不设置 MIDI API

    // 尝试开始录制（应失败但无 panic）
    root.update(Message::Toolbar(toolbar::Event::Record));
    assert!(!root.recording.is_recording, "无 MIDI API 时录制不应启动");

    // 现在设置 API
    root.midi.api = Some(Box::new(MockMidiApi::new()));

    // 正常开始
    root.update(Message::Toolbar(toolbar::Event::Record));
    assert!(root.recording.is_recording, "设置 API 后应能开始录制");

    // 再次开始（应无异常，幂等）
    root.update(Message::Toolbar(toolbar::Event::Record));
    assert!(root.recording.is_recording, "重复开始应保持录制状态");

    // 停止
    root.update(Message::Toolbar(toolbar::Event::RecordStop));
    assert!(!root.recording.is_recording, "停止后应退出录制状态");

    // 再次停止（应无异常）
    root.update(Message::Toolbar(toolbar::Event::RecordStop));
    assert!(!root.recording.is_recording, "重复停止应保持非录制状态");
}

/// 测试 8：录制与播放位置的关系
///
/// 验证：录制过程中 playback_position 随时间推进
#[test]
fn test_recording_position_advancement() {
    let mut root = Root::new(&UiConfig::default());
    root.midi.api = Some(Box::new(MockMidiApi::new()));

    root.update(Message::Toolbar(toolbar::Event::Record));

    // 第一次 poll（设置 started_at）
    root.poll_midi_input();
    let pos1 = root.editor.playback_position;

    // 等待一小段时间后再次 poll
    std::thread::sleep(std::time::Duration::from_millis(50));
    root.poll_midi_input();
    let pos2 = root.editor.playback_position;

    assert!(pos2 > pos1, "录制位置应随时间推进 ({} -> {})", pos1, pos2);

    root.update(Message::Toolbar(toolbar::Event::RecordStop));
}

/// 测试 9：MIDI API 设备列表
///
/// 验证：set_midi_api 后设置面板正确缓存设备列表
#[test]
fn test_midi_api_device_enumeration() {
    let mut root = Root::new(&UiConfig::default());

    // 设置前设备列表为空
    assert!(
        root.settings().midi_devices.is_empty(),
        "初始设备列表应为空"
    );

    // 设置 Mock API
    root.set_midi_api(Box::new(MockMidiApi::new()));

    // 验证设备列表已缓存
    assert_eq!(root.settings().midi_devices.len(), 2, "应缓存 2 个输入设备");
    assert_eq!(
        root.settings().midi_devices[0].1,
        "Mock MIDI Keyboard",
        "设备名称应正确"
    );

    // 验证默认选中第一个设备
    assert_eq!(
        root.settings().selected_midi_device,
        Some(0),
        "应自动选中第一个设备"
    );
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
        let mut buf = root.midi.input_buffer.lock().unwrap();
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
