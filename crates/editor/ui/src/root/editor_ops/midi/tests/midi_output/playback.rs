//! 播放生命周期 / 音符更新 / Host 流 MIDI 输出测试

use crate::editor::note::Note;
use crate::message::Message;
use crate::playback::PlaybackState;
use crate::root::editor_ops::midi::tests::common::{create_mock_output, create_root};
use crate::toolbar;

/// 测试完整的笔记生命周期：
/// 创建笔记 → 设置 MIDI 输出 → 播放 → 停止
#[test]
fn test_full_playback_lifecycle() {
    let mut root = create_root();

    // 添加多个音符
    crate::root::editor_ops::midi::tests::common::attach_test_document(&mut root);
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        Note::new(0.0, 60, 480.0),
    );
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        Note::new(480.0, 64, 240.0),
    );
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        Note::new(720.0, 67, 480.0),
    );

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

    // 验证停止后状态：`manager.state()` 由播放线程写入，属异步状态。
    // 必须用命令队列屏障等 Stop 真正执行完，不能轮询固定次数——固定次数
    // 等价于给播放线程一个墙钟预算，整包并行测试把 CPU 打满时预算会被
    // 击穿，导致「停止后仍为 Playing」的偶发失败。
    if let Some(manager) = &mut root.playback.manager {
        assert!(manager.flush_commands(), "播放线程应在超时内处理完停止命令");
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
    crate::root::editor_ops::midi::tests::common::attach_test_document(&mut root);
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        Note::new(0.0, 60, 480.0),
    );
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

/// 测试 set_midi_output 和 set_playback_midi_output (Host 层) 的一致性
#[test]
fn test_host_set_playback_midi_output_flow() {
    let mut root = create_root();
    crate::root::editor_ops::midi::tests::common::attach_test_document(&mut root);
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        Note::new(0.0, 60, 480.0),
    );

    // Host::set_playback_midi_output → Root::set_midi_output
    root.set_midi_output(create_mock_output());
    assert!(root.playback.pending_midi_output.is_some());

    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.pending_midi_output.is_none());
    assert!(root.playback.manager.is_some());

    root.update(Message::Toolbar(toolbar::Event::Stop));
}
