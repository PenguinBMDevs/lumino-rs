//! CC / PitchBend 事件到达 MIDI 输出的完整链路测试

use crate::editor::note::Note;
use crate::message::Message;
use crate::root::editor_ops::midi::tests::common::{MockOutput, create_root};
use crate::toolbar;
use lumino_note_core::automation::{
    AutomationEvent, AutomationLane, AutomationTarget, SegmentShape,
};
use std::sync::Arc;
use std::sync::atomic::AtomicU32;

/// 测试 CC 事件从 automation_lanes → update_playback_notes → 引擎 → MIDI 输出的完整链路
#[test]
fn test_cc_events_reach_midi_output() {
    let mut root = create_root();

    // 给当前音轨 (track 0) 添加一个音符（音轨 0 需要有音符才能被选中）
    crate::root::editor_ops::midi::tests::common::attach_test_document(&mut root);
    root.editor.editor_state.data.current_track = 0;
    root.editor.editor_state.data.insert_note(
        root.editor.editor_state.data.current_track,
        Note::new(0.0, 60, 480.0),
    );

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
            events: vec![AutomationEvent::new(0, 100, SegmentShape::Step)],
        }));
    root.editor
        .editor_state
        .data
        .automation_lanes
        .push(Arc::new(AutomationLane {
            target: AutomationTarget::CC { controller: 10 },
            track: 0,
            channel: 0,
            events: vec![AutomationEvent::new(480, 64, SegmentShape::Step)],
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
            events: vec![AutomationEvent::new(240, 8192, SegmentShape::Step)], // center
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

    let control_events = lumino_midi_loader::ChunkedList::from_sorted(vec![
        // track 0, ch 0, CC 7 (Volume), val=100, at tick 0
        PackedControlEvent::control_change(0, 0, 0, 7, 100),
        // track 0, ch 0, CC 10 (Pan), val=64, at tick 480
        PackedControlEvent::control_change(480, 0, 0, 10, 64),
        // track 1, ch 1, CC 7, val=80, at tick 0
        PackedControlEvent::control_change(0, 1, 1, 7, 80),
    ]);

    let mut doc = crate::test_helpers::make_test_document();
    // 定制：双轨各 1 音符（不同通道）+ 控制事件 + 总 tick
    doc.notes = vec![
        // track 0: 1 note on channel 0
        lumino_midi_loader::ChunkedList::from_sorted(vec![lumino_midi_loader::NoteEvent {
            start_tick: 0,
            end_tick: 960,
            key: 60,
            velocity: 100,
            channel: 0,
        }]),
        // track 1: 1 note on channel 1
        lumino_midi_loader::ChunkedList::from_sorted(vec![lumino_midi_loader::NoteEvent {
            start_tick: 0,
            end_tick: 960,
            key: 64,
            velocity: 100,
            channel: 1,
        }]),
    ];
    doc.key_signatures = vec![(0, 0, false)];
    doc.control_events = control_events;
    doc.total_ticks = 960;

    // 加载 track 0 的音符（模拟 import 流程）
    let track0_notes = doc.get_track_notes(0);

    // 模拟 MIDI 加载流程
    root.set_midi_document(doc);

    // 此时 automation_lanes 应该已包含 CC 事件
    let lane_count = root.editor.editor_state.data.automation_lanes.len();
    tracing::info!("set_midi_document 后 automation_lanes 数量: {}", lane_count);
    assert_eq!(
        lane_count, 3,
        "应创建 3 个 automation lane (CC7/t0 + CC10/t0 + CC7/t1)"
    );

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
