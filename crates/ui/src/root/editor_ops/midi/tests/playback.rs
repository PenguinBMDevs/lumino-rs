use super::common::*;
use crate::editor::note::Note;
use crate::message::Message;
use crate::toolbar;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

/// 端到端测试：验证引擎线程实际发送了 MIDI note_on 消息
#[test]
fn test_end_to_end_note_actually_plays() {
    let mut root = create_root();

    // 添加一个在 tick 0 的音符
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));

    // 设置 MIDI 输出（使用可计数的 mock）
    let note_on_count = Arc::new(AtomicU32::new(0));
    let note_off_count = Arc::new(AtomicU32::new(0));

    let on_count = Arc::clone(&note_on_count);
    let off_count = Arc::clone(&note_off_count);

    let mock_output = Box::new(MockOutput::with_counters(on_count, off_count));

    root.set_midi_output(mock_output);
    assert!(root.playback.pending_midi_output.is_some());

    // 发送 Play 消息
    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.manager.is_some(), "播放管理器应被创建");
    assert!(
        root.playback.pending_midi_output.is_none(),
        "pending_midi_output 应被消费"
    );
    assert!(root.toolbar.is_playing, "工具栏应标记为 playing");

    // 给引擎线程一点时间启动
    thread::sleep(Duration::from_millis(50));

    // 手动调用 update_playback 来驱动引擎
    for _ in 0..100 {
        root.update_playback();
        thread::sleep(Duration::from_millis(1));
    }

    let on_count = note_on_count.load(Ordering::Relaxed);
    let off_count = note_off_count.load(Ordering::Relaxed);

    tracing::info!("端到端测试: note_on={}, note_off={}", on_count, off_count);

    // 停止播放
    root.update(Message::Toolbar(toolbar::Event::Stop));

    // 断言：至少应该有一个 note_on 被发送
    assert!(
        on_count > 0,
        "端到端播放失败：引擎线程没有发送任何 note_on 消息。\
         这意味着 pending_midi_output 没有被正确传递到引擎线程，\
         或者引擎线程没有正确发送 MIDI 消息。\
         实际 note_on={}, note_off={}",
        on_count,
        off_count
    );
}

/// 模拟真实用户场景：先画音符，再点击播放
#[test]
fn test_draw_note_then_play() {
    let mut root = create_root();

    // 步骤1：设置 MIDI 输出（模拟启动时的初始化）
    let note_on_count = Arc::new(AtomicU32::new(0));
    let note_off_count = Arc::new(AtomicU32::new(0));

    let mock_output = Box::new(MockOutput::with_counters(
        Arc::clone(&note_on_count),
        Arc::clone(&note_off_count),
    ));

    root.set_midi_output(mock_output);
    assert!(
        root.playback.pending_midi_output.is_some(),
        "启动后 pending_midi_output 应有值"
    );

    // 步骤2：用户画一个音符
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    root.editor.mark_notes_changed();

    if root.editor.notes_changed() {
        root.update_playback_notes();
        root.editor.clear_notes_changed();
    }

    // 此时 playback_manager 应该还不存在
    assert!(
        root.playback.manager.is_none(),
        "画音符后不应创建播放管理器"
    );

    // 步骤3：用户点击播放按钮
    root.update(Message::Toolbar(toolbar::Event::Play));

    assert!(
        root.playback.manager.is_some(),
        "点击播放后应创建播放管理器"
    );
    assert!(
        root.playback.pending_midi_output.is_none(),
        "pending_midi_output 应被消费"
    );

    thread::sleep(Duration::from_millis(50));

    for _ in 0..100 {
        root.update_playback();
        thread::sleep(Duration::from_millis(1));
    }

    let on_count = note_on_count.load(Ordering::Relaxed);
    let off_count = note_off_count.load(Ordering::Relaxed);

    tracing::info!(
        "画音符后播放测试: note_on={}, note_off={}",
        on_count,
        off_count
    );

    root.update(Message::Toolbar(toolbar::Event::Stop));

    assert!(
        on_count > 0,
        "画音符后播放失败：没有发送 note_on。实际 note_on={}, note_off={}",
        on_count,
        off_count
    );
}

/// 测试场景：先播放（创建管理器），再画音符，再播放
#[test]
fn test_play_then_draw_then_play() {
    let mut root = create_root();

    // 设置 MIDI 输出
    let note_on_count = Arc::new(AtomicU32::new(0));
    let note_off_count = Arc::new(AtomicU32::new(0));

    let mock_output = Box::new(MockOutput::with_counters(
        Arc::clone(&note_on_count),
        Arc::clone(&note_off_count),
    ));

    root.set_midi_output(mock_output);

    // 步骤1：先添加一个音符并播放（创建播放管理器）
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));
    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.manager.is_some(), "第一次播放应创建管理器");

    // 停止
    root.update(Message::Toolbar(toolbar::Event::Stop));
    thread::sleep(Duration::from_millis(50));

    // 步骤2：再画一个音符
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(480.0, 64, 480.0));
    root.editor.mark_notes_changed();

    if root.editor.notes_changed() {
        root.update_playback_notes();
        root.editor.clear_notes_changed();
    }

    // 步骤3：再次播放
    root.update(Message::Toolbar(toolbar::Event::Play));

    thread::sleep(Duration::from_millis(50));
    for _ in 0..100 {
        root.update_playback();
        thread::sleep(Duration::from_millis(1));
    }

    let on_count = note_on_count.load(Ordering::Relaxed);
    let off_count = note_off_count.load(Ordering::Relaxed);

    tracing::info!(
        "先播再画再播测试: note_on={}, note_off={}",
        on_count,
        off_count
    );

    root.update(Message::Toolbar(toolbar::Event::Stop));

    assert!(
        on_count >= 2,
        "先播再画再播失败：期望至少2个 note_on，实际 note_on={}, note_off={}",
        on_count,
        off_count
    );
}
