use super::common::*;
use crate::editor::note::Note;
use crate::message::Message;
use crate::playback::PlaybackState;
use crate::toolbar;

/// 测试核心契约：
/// set_midi_output 在无 playback_manager 时缓存到 pending_midi_output，
/// Message::Toolbar(toolbar::Event::Play) 消费它并创建 playback_manager
#[test]
fn test_play_consumes_pending_midi_output() {
    let mut root = create_root();

    // 初始状态：干净
    assert!(root.playback.manager.is_none(), "初始无播放管理器");
    assert!(root.playback.pending_midi_output.is_none(), "初始无挂起 MIDI 输出");

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
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    root.set_midi_output(create_mock_output());
    root.update(Message::Toolbar(toolbar::Event::Play));
    root.clear_midi_output();
    assert!(
        root.playback.pending_midi_output.is_none(),
        "有管理器时 clear 后 pending 仍应为 None"
    );

    root.update(Message::Toolbar(toolbar::Event::Stop));
}

/// 测试完整的笔记生命周期：
/// 创建笔记 → 设置 MIDI 输出 → 播放 → 停止
#[test]
fn test_full_playback_lifecycle() {
    let mut root = create_root();

    // 添加多个音符
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(480.0, 64, 240.0));
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(720.0, 67, 480.0));

    // 设置 MIDI 输出
    root.set_midi_output(create_mock_output());
    assert!(root.playback.pending_midi_output.is_some());

    // 播放
    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.manager.is_some(), "播放管理器应被创建");
    assert!(
        root.playback.pending_midi_output.is_none(),
        "pending MIDI 输出应被消费"
    );
    assert!(root.toolbar.is_playing, "播放后工具栏应标记为 playing");

    // 停止
    root.update(Message::Toolbar(toolbar::Event::Stop));

    // 验证停止后状态（manager.state() 是异步的）
    if let Some(ref manager) = root.playback.manager {
        for _ in 0..50 {
            if manager.state() == PlaybackState::Stopped {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_eq!(
            manager.state(),
            PlaybackState::Stopped,
            "停止后应处于 Stopped 状态"
        );
    }
    assert!(!root.toolbar.is_playing, "停止后工具栏 playing 应为 false");
    assert!(
        (root.editor.playback_position - 0.0).abs() < f32::EPSILON,
        "停止后播放位置应重置为 0"
    );
}

/// 测试 update_playback_notes：音符变更后应能更新管理器中的音符
#[test]
fn test_update_playback_notes_after_note_change() {
    let mut root = create_root();

    // 先播放（此时无音符）
    root.set_midi_output(create_mock_output());
    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.manager.is_some());

    // 添加音符并标记变更
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    root.editor.mark_notes_changed();

    // 触发音符更新
    if root.editor.notes_changed() {
        root.update_playback_notes();
        root.editor.clear_notes_changed();
    }

    // 管理器仍存在，pending 无缓存
    assert!(root.playback.manager.is_some());
    assert!(root.playback.pending_midi_output.is_none());

    root.update(Message::Toolbar(toolbar::Event::Stop));
}

/// 测试 load_tempo_changes 在无管理器时缓存
#[test]
fn test_tempo_changes_cached_when_no_manager() {
    let mut root = create_root();

    assert!(root.playback.pending_tempo_changes.is_none());

    root.load_tempo_changes(vec![(0, 500000)]); // 120 BPM
    assert!(
        root.playback.pending_tempo_changes.is_some(),
        "无管理器时 tempo changes 应缓存"
    );

    // 播放时应消费缓存的 tempo changes
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    root.set_midi_output(create_mock_output());
    root.update(Message::Toolbar(toolbar::Event::Play));

    assert!(
        root.playback.pending_tempo_changes.is_none(),
        "播放后 tempo changes 应被消费"
    );
    assert!(root.playback.manager.is_some());

    root.update(Message::Toolbar(toolbar::Event::Stop));
}

/// 测试 set_midi_output 和 set_playback_midi_output (Host 层) 的一致性
#[test]
fn test_host_set_playback_midi_output_flow() {
    let mut root = create_root();
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));

    // Host::set_playback_midi_output → Root::set_midi_output
    root.set_midi_output(create_mock_output());
    assert!(root.playback.pending_midi_output.is_some());

    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.pending_midi_output.is_none());
    assert!(root.playback.manager.is_some());

    root.update(Message::Toolbar(toolbar::Event::Stop));
}
