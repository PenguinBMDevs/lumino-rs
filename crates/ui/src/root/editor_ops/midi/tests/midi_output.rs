use super::common::*;
use crate::editor::note::Note;
use crate::message::Message;
use crate::playback::PlaybackState;
use crate::toolbar;
use lumino_midi_loader::{MidiDocument, TrackManager};
use lumino_note_core::automation::{
    AutomationEvent, AutomationLane, AutomationTarget, SegmentShape,
};
use std::sync::Arc;
use std::sync::atomic::AtomicU32;

/// 测试核心契约：
/// set_midi_output 在无 playback_manager 时缓存到 pending_midi_output，
/// Message::Toolbar(toolbar::Event::Play) 消费它并创建 playback_manager
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

/// 测试 CC 事件从 automation_lanes → update_playback_notes → 引擎 → MIDI 输出的完整链路
#[test]
fn test_cc_events_reach_midi_output() {
    let mut root = create_root();

    // 给当前音轨 (track 0) 添加一个音符（音轨 0 需要有音符才能被选中）
    root.editor
        .editor_state
        .data
        .notes
        .push_back(Note::new(0.0, 60, 480.0));

    // 直接在 automation_lanes 中添加 CC 事件（模拟 MIDI 加载后的状态）
    // CC 7 = Volume, value=100, at tick 0
    // CC 10 = Pan, value=64, at tick 480
    root.editor
        .editor_state
        .data
        .automation_lanes
        .push(Arc::new(AutomationLane {
            target: AutomationTarget::CC { controller: 7 },
            track: 0,
            channel: 0,
            events: vec![AutomationEvent {
                tick: 0,
                value: 100,
                shape: SegmentShape::Step,
            }],
        }));
    root.editor
        .editor_state
        .data
        .automation_lanes
        .push(Arc::new(AutomationLane {
            target: AutomationTarget::CC { controller: 10 },
            track: 0,
            channel: 0,
            events: vec![AutomationEvent {
                tick: 480,
                value: 64,
                shape: SegmentShape::Step,
            }],
        }));

    // 添加 PitchBend 事件
    root.editor
        .editor_state
        .data
        .automation_lanes
        .push(Arc::new(AutomationLane {
            target: AutomationTarget::PitchBend,
            track: 0,
            channel: 0,
            events: vec![AutomationEvent {
                tick: 240,
                value: 8192, // center
                shape: SegmentShape::Step,
            }],
        }));

    // 设置带 CC/PB 计数器的模拟 MIDI 输出
    let cc_count = Arc::new(AtomicU32::new(0));
    let pb_count = Arc::new(AtomicU32::new(0));
    let note_on_count = Arc::new(AtomicU32::new(0));
    let note_off_count = Arc::new(AtomicU32::new(0));

    let mock_output = Box::new(MockOutput::with_all_counters(
        Arc::clone(&note_on_count),
        Arc::clone(&note_off_count),
        Arc::clone(&cc_count),
        Arc::clone(&pb_count),
    ));

    root.set_midi_output(mock_output);
    assert!(root.playback.pending_midi_output.is_some());

    // 播放
    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.manager.is_some());
    assert!(root.playback.pending_midi_output.is_none());

    // 等待引擎处理（模拟多帧播放）
    for _ in 0..200 {
        root.update_playback();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    // 检查计数器：至少应该有 1 个 CC + 1 个 PB
    let actual_cc = cc_count.load(std::sync::atomic::Ordering::Relaxed);
    let actual_pb = pb_count.load(std::sync::atomic::Ordering::Relaxed);
    let actual_note_on = note_on_count.load(std::sync::atomic::Ordering::Relaxed);
    let actual_note_off = note_off_count.load(std::sync::atomic::Ordering::Relaxed);

    tracing::info!(
        "CC播放测试: CC={}, PB={}, NoteOn={}, NoteOff={}",
        actual_cc,
        actual_pb,
        actual_note_on,
        actual_note_off,
    );

    // CC 至少 1 个（tick 0 的 Volume 100 应该立即发送）
    assert!(
        actual_cc >= 1,
        "CC 事件应该至少发送 1 次 (实际 {}) —— update_playback_notes 可能未将 CC 注入引擎、或引擎未处理 midi_events、或消息未到达输出",
        actual_cc,
    );

    // PB 至少 1 个
    assert!(
        actual_pb >= 1,
        "PitchBend 事件应该至少发送 1 次 (实际 {})",
        actual_pb,
    );

    // 停止
    root.update(Message::Toolbar(toolbar::Event::Stop));
}

/// 测试通过 set_midi_document 加载控制事件后，CC 能否正确播放
#[test]
fn test_cc_via_set_midi_document() {
    let mut root = create_root();

    // 构造一个 MidiDocument，包含 CC 事件
    use midly::loader::PackedControlEvent;

    let control_events = vec![
        // track 0, ch 0, CC 7 (Volume), val=100, at tick 0
        PackedControlEvent::control_change(0, 0, 0, 7, 100),
        // track 0, ch 0, CC 10 (Pan), val=64, at tick 480
        PackedControlEvent::control_change(480, 0, 0, 10, 64),
        // track 1, ch 1, CC 7, val=80, at tick 0
        PackedControlEvent::control_change(0, 1, 1, 7, 80),
    ];

    let doc = Arc::new(MidiDocument {
        notes: vec![
            // track 0: 1 note on channel 0
            vec![lumino_midi_loader::NoteEvent {
                start_tick: 0,
                end_tick: 960,
                key: 60,
                velocity: 100,
                channel: 0,
            }],
            // track 1: 1 note on channel 1
            vec![lumino_midi_loader::NoteEvent {
                start_tick: 0,
                end_tick: 960,
                key: 64,
                velocity: 100,
                channel: 1,
            }],
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events,
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("Track 0".into()), Some("Track 1".into())],
        total_ticks: 960,
        track_count: 2,
        tracks: TrackManager::new(2),
        division: 480,
    });

    // 模拟 MIDI 加载流程
    root.set_midi_document(Arc::clone(&doc));

    // 此时 automation_lanes 应该已包含 CC 事件
    let lane_count = root.editor.editor_state.data.automation_lanes.len();
    tracing::info!("set_midi_document 后 automation_lanes 数量: {}", lane_count);
    assert_eq!(
        lane_count, 3,
        "应创建 3 个 automation lane (CC7/t0 + CC10/t0 + CC7/t1)"
    );

    // 验证 lane 的 track 索引
    for lane in &root.editor.editor_state.data.automation_lanes {
        tracing::info!(
            "  lane: track={} target={:?} events={}",
            lane.track,
            lane.target,
            lane.events.len(),
        );
    }

    // 加载 track 0 的音符（模拟 import 流程）
    let track0_notes = doc.get_track_notes(0);
    root.load_track_notes(0, &track0_notes);
    assert_eq!(
        root.editor.editor_state.data.current_track, 0,
        "当前音轨应为 track 0"
    );

    // 验证 automation_lanes 在当前音轨的 CC 事件
    let current_track = root.editor.editor_state.data.current_track as u16;
    let current_lanes: Vec<_> = root
        .editor
        .editor_state
        .data
        .automation_lanes
        .iter()
        .filter(|l| l.track == current_track)
        .collect();
    tracing::info!(
        "当前音轨 {} 的 automation lanes: {}",
        current_track,
        current_lanes.len()
    );
    assert!(!current_lanes.is_empty(), "当前音轨应有 automation lanes");

    // 设置带 CC 计数器的模拟 MIDI 输出
    let cc_count = Arc::new(AtomicU32::new(0));
    let pb_count = Arc::new(AtomicU32::new(0));
    let note_on_count = Arc::new(AtomicU32::new(0));
    let note_off_count = Arc::new(AtomicU32::new(0));

    let mock_output = Box::new(MockOutput::with_all_counters(
        Arc::clone(&note_on_count),
        Arc::clone(&note_off_count),
        Arc::clone(&cc_count),
        Arc::clone(&pb_count),
    ));

    root.set_midi_output(mock_output);
    assert!(root.playback.pending_midi_output.is_some());

    // 播放
    root.update(Message::Toolbar(toolbar::Event::Play));
    assert!(root.playback.manager.is_some());

    // 等待引擎处理
    for _ in 0..200 {
        root.update_playback();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }

    let actual_cc = cc_count.load(std::sync::atomic::Ordering::Relaxed);
    let actual_note_on = note_on_count.load(std::sync::atomic::Ordering::Relaxed);

    tracing::info!(
        "set_midi_document CC测试: CC={}, NoteOn={}",
        actual_cc,
        actual_note_on,
    );

    assert!(
        actual_note_on >= 1,
        "音符应该播放 (实际 NoteOn={})",
        actual_note_on,
    );
    assert!(
        actual_cc >= 1,
        "通过 set_midi_document 加载的 CC 事件应该至少发送 1 次 (实际 {}) \
         —— 检查 set_midi_document 是否正确填充了 automation_lanes，\
         以及 update_playback_notes() 是否从中提取了 CC 事件",
        actual_cc,
    );

    root.update(Message::Toolbar(toolbar::Event::Stop));
}
