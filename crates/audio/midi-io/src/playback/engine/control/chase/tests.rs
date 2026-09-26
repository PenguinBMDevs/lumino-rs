//! seek / 循环回绕状态追齐测试

use super::super::core::PlaybackEngine;
use crate::playback::engine::{MidiMessage, MidiTrackEvent};
use crate::playback::{Playback, PlaybackState};
use lumino_midi_loader::{MidiDocument, NoteEvent as DocNoteEvent, TrackManager};
use parking_lot::Mutex;
use std::sync::Arc;

fn ev(tick: f32, message: MidiMessage) -> MidiTrackEvent {
    MidiTrackEvent { tick, message }
}

fn cc(channel: u8, controller: u8, value: u8) -> MidiMessage {
    MidiMessage::ControlChange {
        channel,
        controller,
        value,
    }
}

fn pb(channel: u8, value: f32) -> MidiMessage {
    MidiMessage::PitchBend { channel, value }
}

fn cc_tuples(messages: &[MidiMessage]) -> Vec<(u8, u8, u8)> {
    messages
        .iter()
        .filter_map(|message| match message {
            MidiMessage::ControlChange {
                channel,
                controller,
                value,
            } => Some((*channel, *controller, *value)),
            _ => None,
        })
        .collect()
}

fn pitch_bends(messages: &[MidiMessage]) -> Vec<(u8, f32)> {
    messages
        .iter()
        .filter_map(|message| match message {
            MidiMessage::PitchBend { channel, value } => Some((*channel, *value)),
            _ => None,
        })
        .collect()
}

/// 构造空音符单轨文档（可带控制事件）。
fn doc_with_control_events(events: Vec<midly::loader::PackedControlEvent>) -> Arc<MidiDocument> {
    Arc::new(MidiDocument {
        next_note_id: 1,
        notes: vec![lumino_midi_loader::ChunkedList::new()],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::from_sorted(events),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![None],
        total_ticks: 0,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: lumino_midi_loader::MidiDocument::new_track_max_ticks(1),
    })
}

fn engine_with_events(events: Vec<MidiTrackEvent>) -> PlaybackEngine {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(playback);
    engine.set_midi_events(events);
    engine
}

/// seek 到 RPN 曲线中段：追齐最后一组 选择+数据，且选择先于数据。
#[test]
fn seek_chases_rpn_cc_and_pitch_bend_state() {
    let mut engine = engine_with_events(vec![
        ev(0.0, cc(0, 101, 0)),
        ev(0.0, cc(0, 100, 1)),
        ev(0.0, cc(0, 6, 32)),
        ev(0.0, cc(0, 38, 0)),
        ev(0.0, cc(0, 7, 100)),
        ev(10.0, pb(0, 0.5)),
        ev(100.0, cc(0, 101, 0)),
        ev(100.0, cc(0, 100, 1)),
        ev(100.0, cc(0, 6, 96)),
        ev(100.0, cc(0, 38, 0)),
        ev(200.0, cc(0, 7, 50)),
    ]);

    engine.seek(150.0);
    let chase = engine.take_pending_chase();

    assert_eq!(
        cc_tuples(&chase),
        vec![
            (0, 101, 0),
            (0, 100, 1),
            (0, 6, 96),
            (0, 38, 0),
            (0, 7, 100),
        ],
        "应追齐 seek 前最后 RPN 选择(101,100) → 数据(6,38) → 其他 CC(tick 150 前 CC7 最后为 tick 0 的 100)，实际 {chase:?}"
    );
    assert_eq!(pitch_bends(&chase), vec![(0, 0.5)]);
}

/// seek 到首个事件之前：没有任何状态可追，返回空。
#[test]
fn seek_before_any_event_yields_empty() {
    let mut engine = engine_with_events(vec![ev(100.0, cc(0, 7, 100))]);
    engine.seek(50.0);
    assert!(engine.take_pending_chase().is_empty());
}

/// 当前轨与 document（其他轨）控制事件按 tick 合并，后出现者覆盖。
#[test]
fn document_events_merge_in_tick_order() {
    let mut engine = engine_with_events(vec![ev(200.0, cc(0, 7, 50))]);
    engine.set_document(
        doc_with_control_events(vec![
            midly::loader::PackedControlEvent::control_change(100, 0, 0, 7, 80),
            midly::loader::PackedControlEvent::control_change(250, 0, 0, 7, 90),
        ]),
        0,
    );

    engine.seek(300.0);
    let chase = engine.take_pending_chase();
    assert_eq!(
        cc_tuples(&chase),
        vec![(0, 7, 90)],
        "tick 250 的 document 事件应覆盖 tick 200 的当前轨事件"
    );
}

/// 最后被选中的参数家族生效：NRPN 之后又选 RPN，则只追 RPN 家族。
#[test]
fn last_selected_parameter_family_wins() {
    let mut engine = engine_with_events(vec![
        ev(0.0, cc(0, 99, 0)),
        ev(0.0, cc(0, 98, 99)),
        ev(0.0, cc(0, 6, 10)),
        ev(100.0, cc(0, 101, 0)),
        ev(100.0, cc(0, 100, 1)),
        ev(100.0, cc(0, 6, 96)),
        ev(100.0, cc(0, 38, 2)),
    ]);

    engine.seek(150.0);
    let chase = engine.take_pending_chase();
    assert_eq!(
        cc_tuples(&chase),
        vec![(0, 101, 0), (0, 100, 1), (0, 6, 96), (0, 38, 2)],
        "最后选择为 RPN，不应追 NRPN(99/98) 的残留值"
    );
}

/// 没有选择状态时不追 DataEntry（目标参数不明确）。
#[test]
fn data_entry_without_selection_is_not_chased() {
    let mut engine = engine_with_events(vec![ev(0.0, cc(0, 6, 10))]);
    engine.seek(100.0);
    assert!(engine.take_pending_chase().is_empty());
}

/// 循环回绕：update 返回的消息里应包含 loop_start 之前的状态追齐。
#[test]
fn loop_wrap_appends_state_chase() {
    let playback = Arc::new(Mutex::new(Playback::new(480)));
    let mut engine = PlaybackEngine::new(Arc::clone(&playback));

    // 文档提供一个可回绕的时间线（音符落在循环范围内）。
    engine.set_document(
        Arc::new(MidiDocument {
            next_note_id: 1,
            notes: vec![lumino_midi_loader::ChunkedList::from_sorted(vec![
                DocNoteEvent::new(60, 70, 60, 100, 0),
            ])],
            tempo_changes: vec![(0, 120.0)],
            time_signatures: vec![(0, 4, 4)],
            key_signatures: vec![],
            control_events: lumino_midi_loader::ChunkedList::new(),
            lyrics: vec![],
            markers: vec![],
            text_events: vec![],
            sys_ex: vec![],
            track_names: vec![None],
            total_ticks: 100,
            track_count: 1,
            tracks: TrackManager::new(1),
            division: 480,
            track_ports: vec![],
            track_max_end_ticks: lumino_midi_loader::MidiDocument::new_track_max_ticks(1),
        }),
        0,
    );
    engine.set_midi_events(vec![ev(0.0, cc(0, 7, 100)), ev(0.0, pb(0, 0.5))]);
    engine.set_looping(true);
    engine.set_loop_range(50.0, 100.0);

    // 建立播放时间基线后跳到 loop_end 之后，再播放触发回绕。
    playback.lock().play();
    std::thread::sleep(std::time::Duration::from_millis(1));
    playback.lock().pause();
    engine.seek(120.0);
    let _ = engine.take_pending_chase();
    engine.play();

    let messages = engine.update();
    let tuples = cc_tuples(messages);
    let bends = pitch_bends(messages);
    assert_eq!(engine.state(), PlaybackState::Playing);
    assert!(
        tuples.contains(&(0, 7, 100)),
        "回绕后应追齐 CC7=100，实际 {tuples:?}"
    );
    assert_eq!(bends, vec![(0, 0.5)]);
}
