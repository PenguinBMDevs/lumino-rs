//! CC / PitchBend 导出链路往返测试（自包含，不依赖 `.mid` 夹具）
//!
//! ## 目的
//!
//! 1. 证明导出链路**同时**承载 CC 与 PitchBend：
//!    - CC 走 `extract_pc_cc_events`（kind 0）；
//!    - PitchBend 走 `extract_passthrough_events`（kind 2）——是**独立通道**。
//!
//!    2026-10-17 数据源冗余审计初稿误以为 `extract.rs` 里的 `_ => {}` 会丢掉
//!    PitchBend；那条分支位于 `extract_pc_cc_events` 内，PB 由 passthrough 通道
//!    处理。本测试是该**误报更正**的可执行证据。
//!
//! 2. 锁定「`MidiDocument` 是 CC/PB 唯一权威」：导出数据完全从文档提取，
//!    与编辑器保存路径（`src/runner/menu/file/editor_midi.rs`）同一数据源。
//!    因此 `EditorData` 的自动化 lane 编辑只要同步进文档，保存/导出即正确。

use lumino_export::midi::{
    MidiExportData, MidiExportOptions, MidiTempoEvent, MidiTrackData, export_midi_to_bytes,
    extract_passthrough_events, extract_pc_cc_events,
};
use lumino_midi_loader::bpm_to_tempo;
use lumino_midi_model::{ChunkedList, MidiDocument, PackedControlEvent, TrackManager};

const TRACK: u16 = 0;
const CHANNEL: u8 = 3;

/// 构造含 CC7 / PitchBend / PC / 通道触后的内存文档（无夹具依赖）
fn doc_with_control_events() -> MidiDocument {
    // 注意顺序即同 tick 顺序：本用例 tick 各不相同，顺序不敏感
    let events = vec![
        PackedControlEvent::program_change(10, TRACK, CHANNEL, 42),
        PackedControlEvent::channel_aftertouch(20, TRACK, CHANNEL, 55),
        PackedControlEvent::control_change(100, TRACK, CHANNEL, 7, 64),
        PackedControlEvent::pitch_bend(200, TRACK, CHANNEL, 12288),
    ];
    MidiDocument {
        notes: vec![ChunkedList::new()],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
        control_events: ChunkedList::from_sorted(events),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![Some("T0".to_string())],
        total_ticks: 1000,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![0],
        track_max_end_ticks: MidiDocument::new_track_max_ticks(1),
    }
}

/// 从文档构造导出数据（走生产级提取函数；仅控制类事件，音符留空）
fn export_data(doc: &MidiDocument) -> MidiExportData {
    let (pc_by_track, cc_by_track) = extract_pc_cc_events(doc);
    let pass = extract_passthrough_events(doc);

    let tracks: Vec<MidiTrackData> = (0..doc.track_count())
        .map(|i| {
            let track_id = i as u16;
            MidiTrackData {
                notes: Vec::new(),
                tempos: if i == 0 {
                    doc.tempo_changes
                        .iter()
                        .map(|&(tick, bpm)| MidiTempoEvent {
                            tick,
                            tempo: bpm_to_tempo(bpm as f64),
                        })
                        .collect()
                } else {
                    Vec::new()
                },
                time_signatures: Vec::new(),
                key_signatures: Vec::new(),
                program_changes: pc_by_track.get(&track_id).cloned().unwrap_or_default(),
                control_changes: cc_by_track.get(&track_id).cloned().unwrap_or_default(),
                pitch_bends: pass.pitch_bends.get(&track_id).cloned().unwrap_or_default(),
                channel_aftertouch: pass
                    .channel_aftertouch
                    .get(&track_id)
                    .cloned()
                    .unwrap_or_default(),
                poly_aftertouch: Vec::new(),
                lyrics: Vec::new(),
                markers: Vec::new(),
                text_events: Vec::new(),
                sys_ex: Vec::new(),
                midi_port: None,
                name: doc.track_name(i).map(|s| s.to_string()),
            }
        })
        .collect();

    MidiExportData {
        options: MidiExportOptions {
            format: 1,
            ppqn: doc.division,
        },
        tracks,
    }
}

/// `PackedControlEvent` 是 packed 结构：字段按值读取，避免未对齐字段引用（E0793）
fn raw_pitch_bend(ev: &PackedControlEvent) -> u16 {
    let bend = (ev.as_pitch_bend() * 8192.0).round() as i32 + 8192;
    bend.clamp(0, 16383) as u16
}

#[test]
fn cc_and_pitchbend_survive_midi_export_roundtrip() {
    let doc = doc_with_control_events();
    let bytes = export_midi_to_bytes(&export_data(&doc)).expect("导出 MIDI 字节应成功");
    let (reloaded, _, _) =
        MidiDocument::from_notes_bytes(&bytes, None).expect("导出的 MIDI 应可重新解析");

    let cc: Vec<(u32, u8, u8)> = reloaded
        .control_events
        .iter()
        .filter(|e| e.kind == 0 && e.track == TRACK)
        .map(|e| {
            let (controller, value) = e.as_control_change();
            (e.tick, controller, value)
        })
        .collect();
    assert_eq!(cc, vec![(100, 7, 64)], "CC7 必须无损往返");

    let pb: Vec<(u32, u16)> = reloaded
        .control_events
        .iter()
        .filter(|e| e.kind == 2 && e.track == TRACK)
        .map(|e| (e.tick, raw_pitch_bend(e)))
        .collect();
    assert_eq!(
        pb,
        vec![(200, 12288)],
        "PitchBend 必须无损往返（经 extract_passthrough_events 通道，不是被跳过）"
    );

    let pc: Vec<(u32, u8)> = reloaded
        .control_events
        .iter()
        .filter(|e| e.kind == 1 && e.track == TRACK)
        .map(|e| (e.tick, e.as_program_change()))
        .collect();
    assert_eq!(pc, vec![(10, 42)], "PC 必须无损往返");

    let at: Vec<(u32, u8)> = reloaded
        .control_events
        .iter()
        .filter(|e| e.kind == 3 && e.track == TRACK)
        .map(|e| (e.tick, e.as_channel_aftertouch()))
        .collect();
    assert_eq!(at, vec![(20, 55)], "通道触后必须无损往返");
}
