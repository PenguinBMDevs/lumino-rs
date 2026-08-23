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
    crate::root::editor_ops::midi::tests::common::attach_test_document(&mut root);
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        Note::new(0.0, 60, 480.0),
    );

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
    crate::root::editor_ops::midi::tests::common::attach_test_document(&mut root);
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        Note::new(0.0, 60, 480.0),
    );
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

/// Root 级胶水测试：经卷帘面板（sidebar）设置独奏，验证最终经默认输出链路过滤播放。
///
/// 覆盖 `update_playback_track_states` 适配器：sidebar.tracks（id 与 document 音轨索引对齐）
/// → 播放状态向量 → 引擎过滤。这是引擎/管理器单测之外唯一尚未端到端覆盖的环节。
#[test]
fn test_solo_via_sidebar_filters_root_playback() {
    let mut root = create_root();

    // 挂载 2 轨 document（当前轨 = 1）
    attach_test_document(&mut root);

    // 用真实路径填充 sidebar.tracks（id 与 document 音轨索引对齐）
    root.sidebar.update_tracks_from_midi(&[
        (0usize, Some("Track 0".to_string()), 0u64, 0u8, 0u8),
        (1usize, Some("Track 1".to_string()), 0u64, 0u8, 0u8),
    ]);

    // 两条轨各写一个 tick=0 的音符（不同键，便于计数区分）
    root.editor
        .editor_state
        .data
        .insert_note(0, Note::new(0.0, 60, 480.0));
    root.editor
        .editor_state
        .data
        .insert_note(1, Note::new(0.0, 64, 480.0));

    // 设置可计数 mock 输出
    let note_on_count = Arc::new(AtomicU32::new(0));
    let note_off_count = Arc::new(AtomicU32::new(0));
    root.set_midi_output(Box::new(MockOutput::with_counters(
        Arc::clone(&note_on_count),
        Arc::clone(&note_off_count),
    )));

    // 先无独奏播放一次（驱动管理器创建），随后停止
    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.manager.is_some(), "播放应创建管理器");
    thread::sleep(Duration::from_millis(50));
    for _ in 0..30 {
        root.update_playback();
        thread::sleep(Duration::from_millis(1));
    }
    root.update(Message::Toolbar(toolbar::Event::Stop));

    // 经卷帘面板独奏轨道 0（当前轨 = 1 不在独奏内）
    root.sidebar.tracks[0].is_soloed = true;
    root.update_playback_track_states();

    // 重新计数并再次播放
    note_on_count.store(0, Ordering::Relaxed);
    note_off_count.store(0, Ordering::Relaxed);
    root.update(Message::Toolbar(toolbar::Event::Play));
    thread::sleep(Duration::from_millis(50));
    for _ in 0..100 {
        root.update_playback();
        thread::sleep(Duration::from_millis(1));
    }
    let on_count = note_on_count.load(Ordering::Relaxed);
    let off_count = note_off_count.load(Ordering::Relaxed);
    root.update(Message::Toolbar(toolbar::Event::Stop));

    tracing::info!(
        "独奏过滤 Root 测试: note_on={}, note_off={}",
        on_count,
        off_count
    );

    assert_eq!(
        on_count, 1,
        "独奏轨道0后，默认输出应仅收到轨道0的 1 个 note_on，轨道1应被过滤。实际 note_on={}, note_off={}",
        on_count, off_count
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
    crate::root::editor_ops::midi::tests::common::attach_test_document(&mut root);
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        Note::new(0.0, 60, 480.0),
    );
    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.manager.is_some(), "第一次播放应创建管理器");

    // 停止
    root.update(Message::Toolbar(toolbar::Event::Stop));
    thread::sleep(Duration::from_millis(50));

    // 步骤2：再画一个音符（document 已挂载，直接插入）
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        Note::new(480.0, 64, 480.0),
    );
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
