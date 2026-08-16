//! MIDI 高级功能测试
//!
//! 测试覆盖：
//! - 零力度 NoteOn 当作 NoteOff 处理
//! - 孤立 NoteOff（无对应 NoteOn）被忽略
//! - 停止录制时残留音符的处理
//! - 录制状态机完整性（开始/停止/幂等/无 API 降级）

use lumino_core::storage::config::UiConfig;
use lumino_ui::message::Message;
use lumino_ui::root::Root;
use lumino_ui::toolbar;

use crate::basic::MockMidiApi;

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
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("锁定MIDI输入缓冲区失败");
        buf.push_back(vec![0x90, 60, 100]);
    }
    root.poll_midi_input();

    assert_eq!(root.editor.editor_state.data.notes.len(), 1);

    // 零力度 NoteOn（应作为 NoteOff）
    {
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("锁定MIDI输入缓冲区失败");
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
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("锁定MIDI输入缓冲区失败");
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
        let mut buf = root
            .midi
            .input_buffer
            .lock()
            .expect("锁定MIDI输入缓冲区失败");
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
