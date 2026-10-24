//! `Root::set_midi_document` 装载契约测试
//!
//! 审计 §五：播放键色索引此前用「文档内存地址」判失效 —— 旧文档 drop 后
//! 新文档**可能分配到同一地址**，于是一份全新文档被判为「同一份」，键色残留
//! 上一份内容。改用 `EditorData::track_notes_gen`（音符内容世代）后，
//! 「装载文档 = 世代变化」成为所有派生缓存失效的硬前提，本测试锁定它。

use crate::root::Root;
use crate::test_helpers::event_queue_lock;
use lumino_core::storage::config::UiConfig;
use lumino_midi_loader::{ChunkedList, MidiDocument, NoteEvent, TrackManager};

/// 单轨单音符文档（`0..1920`）
fn single_note_doc(key: u8) -> MidiDocument {
    MidiDocument {
        notes: vec![ChunkedList::from_sorted(vec![NoteEvent::new(
            0, 1920, key, 100, 0,
        )])],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
        control_events: ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![None],
        total_ticks: 1920,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![0],
        track_max_end_ticks: MidiDocument::new_track_max_ticks(1),
        track_max_note_lens: MidiDocument::new_track_max_note_lens_unknown(1),
    }
}

/// 每次装载文档都必须 bump 音符世代（否则派生缓存沿用上一份文档的内容）
#[test]
fn test_set_midi_document_bumps_notes_generation() {
    let _guard = event_queue_lock();
    let _ = lumino_message::events::take_events();

    let ui_config = UiConfig::default();
    let mut root = Root::new(&ui_config);

    root.set_midi_document(single_note_doc(60));
    let after_first = root.editor.editor_state.data.track_notes_gen;

    // 连续装载第二份内容不同的文档：地址可能被复用，世代必须变化
    root.set_midi_document(single_note_doc(62));
    let after_second = root.editor.editor_state.data.track_notes_gen;

    assert!(
        after_second > after_first,
        "装载文档必须 bump 音符世代：{after_first} -> {after_second}"
    );

    // 内容确实换成新文档（世代之外再核一遍数据本身）
    let doc = root
        .editor
        .editor_state
        .data
        .document
        .as_ref()
        .expect("文档应存在");
    assert_eq!(
        doc.notes[0].iter().next().map(|n| n.key),
        Some(62),
        "应装载第二份文档"
    );
}
