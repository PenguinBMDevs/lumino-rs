//! 文档加载与查询测试

use super::common::create_simple_midi_bytes;
use crate::MidiDocument;

#[test]
fn test_from_notes_file() {
    let bytes = create_simple_midi_bytes();
    let doc_path = std::env::temp_dir().join("doc_test.mid");
    std::fs::write(&doc_path, &bytes).expect("测试：写入临时文件失败");

    let doc = MidiDocument::from_notes_file(&doc_path, None).expect("测试：加载MIDI文档失败");
    assert_eq!(doc.track_count(), 1);
    assert!(doc.total_ticks > 0);
    assert!(!doc.notes.is_empty());
    assert!(!doc.notes[0].is_empty());

    let evs = doc.get_track_events(0);
    assert!(!evs.is_empty());

    let notes = doc.get_track_notes(0);
    assert!(!notes.is_empty());

    let _ = std::fs::remove_file(&doc_path);
}

#[test]
fn test_get_events_in_range() {
    let bytes = create_simple_midi_bytes();
    let range_path = std::env::temp_dir().join("doc_range.mid");
    std::fs::write(&range_path, &bytes).expect("测试：写入临时文件失败");

    let doc = MidiDocument::from_notes_file(&range_path, None).expect("测试：加载MIDI文档失败");
    let events = doc.get_events_in_range(0, 1000, 0);
    assert!(!events.is_empty());

    let empty = doc.get_events_in_range(99999, 100000, 0);
    assert!(empty.is_empty());

    let _ = std::fs::remove_file(&range_path);
}

#[test]
fn test_track_notes_contiguous_range() {
    let bytes = create_simple_midi_bytes();
    let contig_path = std::env::temp_dir().join("doc_contig.mid");
    std::fs::write(&contig_path, &bytes).expect("测试：写入临时文件失败");

    let doc = MidiDocument::from_notes_file(&contig_path, None).expect("测试：加载MIDI文档失败");
    let evs = doc.get_track_events(0);
    for ev in &evs {
        assert_eq!(
            ev.track_id(),
            0,
            "all events in get_track_events(0) must have track_id=0"
        );
    }

    let _ = std::fs::remove_file(&contig_path);
}

/// 构造含 MidiPort meta (FF 21) 的最小 SMF 字节（1 轨、1 个音符、端口=1）
fn midi_bytes_with_port(port: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    // MThd: format 0, 1 track, division 480
    bytes.extend_from_slice(b"MThd");
    bytes.extend_from_slice(&[0, 0, 0, 6, 0, 0, 0, 1, 1, 0xE0]);
    // MTrk
    bytes.extend_from_slice(b"MTrk");
    let track_events: Vec<u8> = {
        let mut ev = Vec::new();
        // delta 0 + FF 21 01 <port> (MidiPort meta)
        ev.extend_from_slice(&[0x00, 0xFF, 0x21, 0x01, port]);
        // delta 0 + NoteOn (ch0, key 60, vel 100)
        ev.extend_from_slice(&[0x00, 0x90, 0x3C, 0x64]);
        // delta 96 + NoteOff
        ev.extend_from_slice(&[0x60, 0x80, 0x3C, 0x40]);
        // delta 0 + EndOfTrack
        ev.extend_from_slice(&[0x00, 0xFF, 0x2F, 0x00]);
        ev
    };
    let len = track_events.len() as u32;
    bytes.extend_from_slice(&len.to_be_bytes());
    bytes.extend_from_slice(&track_events);
    bytes
}

/// 2026-08-15 回归测试：MidiPort meta (FF 21) 由流式解析直接提取
/// （此前依赖 `Smf::parse` 对文件做第二次全量解析，2.9 亿音符黑乐谱
/// 场景产生 15-18GB 临时峰值；现经 lumino-midly 0.6.3 流式提取）
#[test]
fn test_midi_port_extracted_streaming() {
    let bytes = midi_bytes_with_port(7);
    let (doc, _, _) = MidiDocument::from_notes_bytes(&bytes, None).expect("测试：解析MIDI失败");
    assert_eq!(doc.track_count(), 1);
    assert_eq!(doc.track_port(0), 7, "FF 21 meta 应被提取为音轨端口");
}

/// 无 MidiPort meta 时端口默认 0（兼容旧行为）
#[test]
fn test_midi_port_defaults_zero() {
    let bytes = create_simple_midi_bytes();
    let (doc, _, _) = MidiDocument::from_notes_bytes(&bytes, None).expect("测试：解析MIDI失败");
    assert_eq!(doc.track_port(0), 0, "无 FF 21 meta 时端口应为 0");
}
