//! MIDI 导出 —— 综合事件（ProgramChange + ControlChange）导出测试子集
//!
//! 从 `midi/tests.rs` 拆分而来。

use crate::midi::{
    MidiControlChangeEvent, MidiExportData, MidiExportOptions, MidiNoteEvent,
    MidiProgramChangeEvent, MidiTrackData, export_midi_to_bytes,
};
use midly::{MidiMessage, Smf, TrackEventKind};

#[test]
fn test_export_midi_with_pc_and_cc() {
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
            program: 0,
        }],
        control_changes: vec![MidiControlChangeEvent {
            tick: 0,
            channel: 0,
            controller: 7,
            value: 100,
        }],
        time_signatures: vec![],
        key_signatures: vec![],
        name: Some(String::from("Test")),
    };
    let export_data = MidiExportData {
        options: MidiExportOptions {
            format: 1,
            ppqn: 480,
        },
        tracks: vec![track],
    };
    let bytes = export_midi_to_bytes(&export_data).expect("export should succeed");
    let smf = Smf::parse(&bytes).expect("should parse exported MIDI");
    assert_eq!(smf.tracks.len(), 1);

    let mut found_pc = false;
    let mut found_cc = false;
    for event in &smf.tracks[0] {
        match &event.kind {
            TrackEventKind::Midi {
                message: MidiMessage::ProgramChange { .. },
                ..
            } => found_pc = true,
            TrackEventKind::Midi {
                message: MidiMessage::Controller { .. },
                ..
            } => found_cc = true,
            _ => {}
        }
    }
    assert!(found_pc, "should contain ProgramChange");
    assert!(found_cc, "should contain Controller");
}

#[test]
fn test_export_midi_format0_with_pc_cc() {
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
            program: 24,
        }],
        control_changes: vec![MidiControlChangeEvent {
            tick: 0,
            channel: 0,
            controller: 7,
            value: 80,
        }],
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
    let bytes = export_midi_to_bytes(&export_data).expect("export should succeed");
    let smf = Smf::parse(&bytes).expect("should parse exported MIDI");
    assert_eq!(smf.tracks.len(), 1, "format 0 should have 1 track");

    let mut found_pc = false;
    let mut found_cc = false;
    for event in &smf.tracks[0] {
        match &event.kind {
            TrackEventKind::Midi {
                message: MidiMessage::ProgramChange { program },
                ..
            } => {
                assert_eq!(u8::from(*program), 24);
                found_pc = true;
            }
            TrackEventKind::Midi {
                message: MidiMessage::Controller { controller, value },
                ..
            } => {
                assert_eq!(u8::from(*controller), 7);
                assert_eq!(u8::from(*value), 80);
                found_cc = true;
            }
            _ => {}
        }
    }
    assert!(found_pc, "format 0 should contain ProgramChange");
    assert!(found_cc, "format 0 should contain Controller");
}
