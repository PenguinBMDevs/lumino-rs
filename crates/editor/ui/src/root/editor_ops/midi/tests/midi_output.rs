use super::common::*;
use crate::editor::note::Note;
use crate::message::Message;
use crate::toolbar;

/// 测试核心契约：
/// set_midi_output 在无 playback_manager 时缓存到 pending_midi_output，
/// Message::Toolbar(toolbar::Event::Play) 消费它并创建 playback_manager
///
/// 拆分说明（避免单文件超 400 行）：
/// - `midi_output/playback.rs`：完整播放生命周期 / 音符更新 / Host 流
/// - `midi_output/tempo.rs`：tempo changes 与 BPM 缓存路径
/// - `midi_output/cc.rs`：CC/PitchBend 事件到达 MIDI 输出链路
#[test]
fn test_play_consumes_pending_midi_output() {
    let mut root = create_root();

    // 初始状态：干净
    assert!(root.playback.manager.is_none(), "初始无播放管理器");
    assert!(
        root.playback.pending_midi_output.is_none(),
        "初始无挂起 MIDI 输出"
    );

    add_two_test_notes(&mut root);

    // 设置 MIDI 输出 → 因无管理器应缓存
    root.set_midi_output(create_mock_output());
    assert!(
        root.playback.pending_midi_output.is_some(),
        "无播放管理器时 set_midi_output 应缓存到 pending_midi_output"
    );
    assert!(
        root.playback.manager.is_none(),
        "pending 状态不应创建播放管理器"
    );

    // 发送 Play 消息 → 应消费缓存并创建管理器
    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.manager.is_some(), "Play 消息应创建播放管理器");
    assert!(
        root.playback.pending_midi_output.is_none(),
        "Play 消息应消费 pending_midi_output"
    );

    // toolbar.is_playing 是同步设置的（在 manager.play() 之前），可立即验证
    assert!(root.toolbar.is_playing, "Play 后工具栏 playing 应为 true");

    // 停止并自动清理
    root.update(Message::Toolbar(toolbar::Event::Stop));
    assert!(!root.toolbar.is_playing, "Stop 后工具栏 playing 应为 false");
}

/// 测试已存在播放管理器时，set_midi_output 直接传递不缓存
#[test]
fn test_set_midi_output_direct_when_manager_exists() {
    let mut root = create_root();
    add_two_test_notes(&mut root);

    // 先通过 Play 创建 playback_manager
    root.set_midi_output(create_mock_output());
    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.manager.is_some());
    assert!(root.playback.pending_midi_output.is_none());

    // 此时再调用 set_midi_output：应直接传递给 manager，不缓存
    root.set_midi_output(create_mock_output());
    assert!(
        root.playback.pending_midi_output.is_none(),
        "有播放管理器时不应缓存 MIDI output"
    );

    root.update(Message::Toolbar(toolbar::Event::Stop));
}

/// 测试 clear_midi_output 的完整性：清除缓存 + 转发到管理器
#[test]
fn test_clear_midi_output_clears_pending() {
    let mut root = create_root();

    // 仅缓存（无管理器）
    root.set_midi_output(create_mock_output());
    assert!(root.playback.pending_midi_output.is_some());

    root.clear_midi_output();
    assert!(
        root.playback.pending_midi_output.is_none(),
        "clear_midi_output 应清除 pending"
    );

    // 有管理器时
    crate::root::editor_ops::midi::tests::common::attach_test_document(&mut root);
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        Note::new(0.0, 60, 480.0),
    );
    root.set_midi_output(create_mock_output());
    root.update(Message::Toolbar(toolbar::Event::Play));
    root.clear_midi_output();
    assert!(
        root.playback.pending_midi_output.is_none(),
        "有管理器时 clear 后 pending 仍应为 None"
    );

    root.update(Message::Toolbar(toolbar::Event::Stop));
}

mod cc;
mod playback;
mod tempo;
