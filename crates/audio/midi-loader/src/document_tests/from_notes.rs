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
