mod combined;

use super::tracks::convert_to_delta_times;
use super::*;
use midly::{MetaMessage, TrackEvent, TrackEventKind};

#[test]
fn test_export_empty_midi() {
    // 空轨道应导出为有效的 MIDI 文件
    let export_data = MidiExportData {
        options: MidiExportOptions {
            format: 1,
            ppqn: 480,
        },
        tracks: vec![],
    };
    let exported = export_midi_to_bytes(&export_data);
    assert!(exported.is_ok(), "empty MIDI should export successfully");
    let bytes = exported.expect("导出空MIDI数据失败");
    // MIDI 文件头: "MThd" + 6 bytes header
    assert!(bytes.len() >= 14, "MIDI header should be at least 14 bytes");
    assert_eq!(&bytes[0..4], b"MThd", "should start with MThd");
}

#[test]
fn test_export_single_note_midi() {
    let note = MidiNoteEvent {
        tick: 0,
        channel: 0,
        key: 60,
        velocity: 100,
        duration: 480,
    };
    let track = MidiTrackData {
        notes: vec![note],
        tempos: vec![],
        program_changes: vec![],
        control_changes: vec![],
        pitch_bends: vec![],
        time_signatures: vec![],
        key_signatures: vec![],
        name: Some(String::from("Test Track")),
    };
    let export_data = MidiExportData {
        options: MidiExportOptions {
            format: 1,
            ppqn: 480,
        },
        tracks: vec![track],
    };
    let exported = export_midi_to_bytes(&export_data);
    assert!(
        exported.is_ok(),
        "single note MIDI should export successfully"
    );
    let bytes = exported.expect("导出单音符MIDI数据失败");
    assert_eq!(&bytes[0..4], b"MThd", "should start with MThd");
    // 格式 1 应包含轨道数据
    assert!(bytes.len() > 14, "should contain track data beyond header");
}

#[test]
fn test_export_format0_single_track() {
    let note = MidiNoteEvent {
        tick: 0,
        channel: 0,
        key: 60,
        velocity: 100,
        duration: 480,
    };
    let track = MidiTrackData {
        notes: vec![note],
        tempos: vec![MidiTempoEvent {
            tick: 0,
            tempo: 500000,
        }],
        program_changes: vec![],
        control_changes: vec![],
        pitch_bends: vec![],
        time_signatures: vec![],
        key_signatures: vec![],
        name: None,
    };
    let export_data = MidiExportData {
        options: MidiExportOptions {
            format: 0,
            ppqn: 480,
        },
        tracks: vec![track],
    };
    let exported = export_midi_to_bytes(&export_data);
    assert!(exported.is_ok(), "format 0 MIDI should export successfully");
    let bytes = exported.expect("导出Format 0 MIDI数据失败");
    assert_eq!(&bytes[0..4], b"MThd", "should start with MThd");
    // Format 0: 单个轨道
    assert_eq!(bytes[10], 0, "format 0 should have 1 track (high byte)");
    assert_eq!(bytes[11], 1, "format 0 should have 1 track (low byte)");
}

#[test]
fn test_bpm_conversion_roundtrip() {
    let bpm = 120.0;
    let tempo = bpm_to_tempo(bpm);
    let recovered = tempo_to_bpm(tempo);
    assert!(
        (recovered - bpm).abs() < 0.01,
        "BPM roundtrip should be precise"
    );
}

#[test]
fn test_convert_to_delta_times() {
    use midly::num::u28;
    let mut events = vec![
        TrackEvent {
            delta: u28::from(100u32),
            kind: TrackEventKind::Meta(MetaMessage::TrackName(b"foo")),
        },
        TrackEvent {
            delta: u28::from(50u32),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        },
    ];
    convert_to_delta_times(&mut events);
    // After sorting: 50 should be first with delta 50, then 100 with delta 50
    assert_eq!(
        u32::from(events[0].delta),
        50,
        "first event delta should be 50"
    );
    assert_eq!(
        u32::from(events[1].delta),
        50,
        "second event delta should be 50"
    );
}

#[test]
fn test_convert_to_delta_times_empty() {
    let mut events: Vec<TrackEvent<'_>> = vec![];
    convert_to_delta_times(&mut events);
    assert!(events.is_empty(), "empty events should remain empty");
}

#[test]
fn test_build_smf_with_track_name() {
    let track = MidiTrackData {
        notes: vec![],
        tempos: vec![],
        program_changes: vec![],
        control_changes: vec![],
        pitch_bends: vec![],
        time_signatures: vec![],
        key_signatures: vec![],
        name: Some(String::from("Piano")),
    };
    let export_data = MidiExportData {
        options: MidiExportOptions {
            format: 1,
            ppqn: 480,
        },
        tracks: vec![track],
    };
    let bytes = export_midi_to_bytes(&export_data).expect("export should succeed for valid data");
    // 用 midly 重新解析验证输出有效性
    let smf = midly::Smf::parse(&bytes).expect("should parse exported MIDI");
    assert_eq!(smf.tracks.len(), 1, "should have 1 track");
    // 第一个轨道事件应该是 TrackName meta 事件
    if let Some(first_event) = smf.tracks[0].first() {
        match &first_event.kind {
            TrackEventKind::Meta(MetaMessage::TrackName(name)) => {
                assert_eq!(name, b"Piano");
            }
            _ => panic!("first event should be TrackName"),
        }
    } else {
        panic!("track should have events");
    }
}

#[test]
fn test_export_midi_with_program_change() {
    let track = MidiTrackData {
        notes: vec![MidiNoteEvent {
            tick: 0,
            channel: 0,
            key: 60,
            velocity: 100,
            duration: 480,
        }],
        tempos: vec![],
        program_changes: vec![MidiProgramChangeEvent {
            tick: 0,
            channel: 0,
            program: 5, // Acoustic Grand Piano
        }],
        control_changes: vec![],
        pitch_bends: vec![],
        time_signatures: vec![],
        key_signatures: vec![],
        name: None,
    };
    let export_data = MidiExportData {
        options: MidiExportOptions {
            format: 1,
            ppqn: 480,
        },
        tracks: vec![track],
    };
    let bytes = export_midi_to_bytes(&export_data).expect("export should succeed");
    let smf = midly::Smf::parse(&bytes).expect("should parse exported MIDI");
    assert_eq!(smf.tracks.len(), 1);

    // 验证 ProgramChange 事件存在
    let mut found_pc = false;
    for event in &smf.tracks[0] {
        if let TrackEventKind::Midi {
            channel,
            message: midly::MidiMessage::ProgramChange { program },
        } = &event.kind
        {
            assert_eq!(u8::from(*channel), 0);
            assert_eq!(u8::from(*program), 5);
            found_pc = true;
        }
    }
    assert!(found_pc, "exported MIDI should contain ProgramChange event");
}

#[test]
fn test_export_midi_with_control_change() {
    let track = MidiTrackData {
        notes: vec![MidiNoteEvent {
            tick: 0,
            channel: 0,
            key: 60,
            velocity: 100,
            duration: 480,
        }],
        tempos: vec![],
        program_changes: vec![],
        control_changes: vec![
            MidiControlChangeEvent {
                tick: 0,
                channel: 0,
                controller: 7, // Volume
                value: 100,
            },
            MidiControlChangeEvent {
                tick: 480,
                channel: 0,
                controller: 10, // Pan
                value: 64,
            },
        ],
        pitch_bends: vec![],
        time_signatures: vec![],
        key_signatures: vec![],
        name: None,
    };
    let export_data = MidiExportData {
        options: MidiExportOptions {
            format: 1,
            ppqn: 480,
        },
        tracks: vec![track],
    };
    let bytes = export_midi_to_bytes(&export_data).expect("export should succeed");
    let smf = midly::Smf::parse(&bytes).expect("should parse exported MIDI");
    assert_eq!(smf.tracks.len(), 1);

    // 验证 Controller 事件存在
    let mut found_cc7 = false;
    let mut found_cc10 = false;
    for event in &smf.tracks[0] {
        if let TrackEventKind::Midi {
            channel,
            message: midly::MidiMessage::Controller { controller, value },
        } = &event.kind
        {
            assert_eq!(u8::from(*channel), 0);
            match u8::from(*controller) {
                7 => {
                    assert_eq!(u8::from(*value), 100);
                    found_cc7 = true;
                }
                10 => {
                    assert_eq!(u8::from(*value), 64);
                    found_cc10 = true;
                }
                _ => {}
            }
        }
    }
    assert!(found_cc7, "exported MIDI should contain CC7 (Volume)");
    assert!(found_cc10, "exported MIDI should contain CC10 (Pan)");
}
