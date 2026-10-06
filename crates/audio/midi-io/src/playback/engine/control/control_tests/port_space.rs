//! REND-002 / #102：当前轨队列的端口 → 全局通道映射与 `set_document` 刷新语义
//!
//! 锁死 UI 端口编辑链路依赖的引擎契约：`set_document` 收到新快照后必须按
//! 新 `track_ports` 重建当前轨队列（否则音符仍走旧端口，与 CC/PC 分裂）。

use super::super::core::PlaybackEngine;
use crate::playback::Playback;
use crate::playback::engine::EventType;
use lumino_midi_loader::{MidiDocument, NoteEvent as DocNoteEvent, TrackManager};
use parking_lot::Mutex;
use std::sync::Arc;

/// 单轨文档：ch3 一颗音符（tick 0..480），轨端口 = `port`。
fn doc_with_ports(port: u8) -> Arc<MidiDocument> {
    let notes = vec![DocNoteEvent::new(0, 480, 60, 100, 3)];
    Arc::new(MidiDocument {
        notes: vec![lumino_midi_loader::ChunkedList::from_sorted(notes)],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_loader::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![None],
        total_ticks: 480,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![port],
        track_max_end_ticks: lumino_midi_loader::MidiDocument::new_track_max_ticks(1),
    })
}

fn new_engine() -> PlaybackEngine {
    PlaybackEngine::new(Arc::new(Mutex::new(Playback::new(480))))
}

/// 队列中全部音符事件的通道序列（NoteOn/NoteOff 各一条）。
fn queue_channels(engine: &PlaybackEngine) -> Vec<u16> {
    engine
        .event_queue
        .iter()
        .map(|event| match &event.event_type {
            EventType::NoteOn { channel, .. } | EventType::NoteOff { channel, .. } => *channel,
        })
        .collect()
}

/// 单端口（port=0）：队列通道恒等（零行为变化基线）。
#[test]
fn queue_channels_identity_for_single_port() {
    let mut engine = new_engine();
    engine.set_document(doc_with_ports(0), 0);

    assert_eq!(
        queue_channels(&engine),
        vec![3, 3],
        "单端口下 NoteOn/NoteOff 应保持 ch3 恒等"
    );
}

/// 多端口：当前轨事件映射到 `port*16+channel`。
#[test]
fn queue_channels_map_track_port_to_global_channel() {
    let mut engine = new_engine();
    engine.set_document(doc_with_ports(1), 0);

    assert_eq!(
        queue_channels(&engine),
        vec![16 + 3, 16 + 3],
        "端口 1 ch3 应映射到全局通道 19"
    );
}

/// #102 Critical-1 引擎侧契约：端口编辑后以新快照 `set_document`，
/// 队列必须按新端口重建（UI 侧 refurbish 依赖此语义）。
#[test]
fn set_document_refreshes_queue_after_port_change() {
    let mut engine = new_engine();
    engine.set_document(doc_with_ports(0), 0);
    assert_eq!(queue_channels(&engine), vec![3, 3]);

    // 同一音符、端口 0 -> 2 的新快照（模拟端口编辑后的文档）
    engine.set_document(doc_with_ports(2), 0);
    assert_eq!(
        queue_channels(&engine),
        vec![32 + 3, 32 + 3],
        "新快照必须重建队列映射，否则音符仍走旧端口"
    );
}
