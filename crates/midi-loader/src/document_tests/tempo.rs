//! Tempo 变化测试

use crate::MidiDocument;

#[test]
fn test_tempo_changes_uses_file_tempo_at_tick_zero() {
    let header = [
        0x4D, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x01, 0xE0,
    ];
    let track_data = [
        0x00, 0xFF, 0x51, 0x03, 0x06, 0x8A, 0x1B, 0x00, 0x90, 0x3C, 0x64, 0x83, 0x60, 0x80, 0x3C,
        0x00, 0x00, 0xFF, 0x2F, 0x00,
    ];
    let mut track_chunk = vec![0x4D, 0x54, 0x72, 0x6B];
    let track_len = (track_data.len() as u32).to_be_bytes();
    track_chunk.extend_from_slice(&track_len);
    track_chunk.extend_from_slice(&track_data);

    let mut midi = Vec::new();
    midi.extend_from_slice(&header);
    midi.extend_from_slice(&track_chunk);

    let tmp = std::env::temp_dir().join("tempo_140_test.mid");
    std::fs::write(&tmp, &midi).expect("测试：写入临时文件失败");

    let doc = MidiDocument::from_notes_file(&tmp, None).expect("测试：加载MIDI文档失败");

    assert!(!doc.tempo_changes.is_empty(), "应有 tempo 变化");
    let (first_tick, first_bpm) = doc.tempo_changes[0];
    assert_eq!(first_tick, 0, "第一个 tempo 事件应在 tick 0");
    assert!(
        (first_bpm - 140.0).abs() < 0.5,
        "tempo 应为 ~140 BPM，实际为 {first_bpm}"
    );
    assert!(
        doc.tempo_changes.iter().all(|(_, b)| *b > 0.0),
        "所有 tempo 值必须大于 0"
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_tempo_changes_default_120_when_no_tick_zero_tempo() {
    let header = [
        0x4D, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x01, 0xE0,
    ];
    let track_data = [
        0x00, 0x90, 0x3C, 0x64, 0x83, 0x60, 0x80, 0x3C, 0x00, 0x00, 0xFF, 0x2F, 0x00,
    ];
    let mut track_chunk = vec![0x4D, 0x54, 0x72, 0x6B];
    let track_len = (track_data.len() as u32).to_be_bytes();
    track_chunk.extend_from_slice(&track_len);
    track_chunk.extend_from_slice(&track_data);

    let mut midi = Vec::new();
    midi.extend_from_slice(&header);
    midi.extend_from_slice(&track_chunk);

    let tmp = std::env::temp_dir().join("tempo_default_test.mid");
    std::fs::write(&tmp, &midi).expect("测试：写入临时文件失败");

    let doc = MidiDocument::from_notes_file(&tmp, None).expect("测试：加载MIDI文档失败");

    assert!(!doc.tempo_changes.is_empty(), "应有默认 tempo");
    let (first_tick, first_bpm) = doc.tempo_changes[0];
    assert_eq!(first_tick, 0, "默认 tempo 应在 tick 0");
    assert!(
        (first_bpm - 120.0).abs() < 0.5,
        "应为默认 120 BPM，实际为 {first_bpm}"
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn test_tempo_changes_multiple_changes() {
    let header = [
        0x4D, 0x54, 0x68, 0x64, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x01, 0x01, 0xE0,
    ];
    let track_data = [
        0x00, 0xFF, 0x51, 0x03, 0x06, 0x8A, 0x1B, 0x00, 0x90, 0x3C, 0x64, 0x83, 0x60, 0x80, 0x3C,
        0x00, 0x83, 0x60, 0xFF, 0x51, 0x03, 0x0B, 0x71, 0xC0, 0x00, 0xFF, 0x2F, 0x00,
    ];
    let mut track_chunk = vec![0x4D, 0x54, 0x72, 0x6B];
    let track_len = (track_data.len() as u32).to_be_bytes();
    track_chunk.extend_from_slice(&track_len);
    track_chunk.extend_from_slice(&track_data);

    let mut midi = Vec::new();
    midi.extend_from_slice(&header);
    midi.extend_from_slice(&track_chunk);

    let tmp = std::env::temp_dir().join("tempo_multi_test.mid");
    std::fs::write(&tmp, &midi).expect("测试：写入临时文件失败");

    let doc = MidiDocument::from_notes_file(&tmp, None).expect("测试：加载MIDI文档失败");

    assert_eq!(doc.tempo_changes.len(), 2, "应有 2 个 tempo 变化");
    let (t0, b0) = doc.tempo_changes[0];
    assert_eq!(t0, 0);
    assert!((b0 - 140.0).abs() < 0.5, "第一段应为 140 BPM，实际为 {b0}");
    let (t1, b1) = doc.tempo_changes[1];
    assert_eq!(t1, 960);
    assert!((b1 - 80.0).abs() < 0.5, "第二段应为 80 BPM，实际为 {b1}");

    let _ = std::fs::remove_file(&tmp);
}
