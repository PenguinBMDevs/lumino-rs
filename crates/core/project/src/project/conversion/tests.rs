use super::*;
use lumino_midi_model::compact::{CompactEvent, EventKind};

fn make_test_document() -> MidiDocument {
    MidiDocument { next_note_id: 1,
        notes: vec![lumino_midi_model::ChunkedList::from_sorted(vec![
            NoteEvent::new(0, 480, 60, 100, 0),
        ])],
        time_signatures: vec![(0, 4, 4)],
        tempo_changes: vec![(0, 120.0)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_model::ChunkedList::from_sorted(vec![
            midly::loader::PackedControlEvent::control_change(0, 0, 0, 7, 100),
        ]),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("Piano".into())],
        total_ticks: 480,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![],

        track_max_end_ticks: vec![],
    }
}

#[test]
fn test_from_midi_document() {
    let doc = make_test_document();
    let project = LuminoProject::from_midi_document(&doc);

    assert_eq!(project.metadata.audio.total_ticks, 480);
    assert_eq!(project.metadata.audio.track_count, 1);
    assert_eq!(project.tempo_changes.len(), 1);
    assert_eq!(project.control_changes.len(), 1);
    assert_eq!(project.loaded_track_count(), 1);

    let track = project.get_track(0).expect("音轨 0 应已加载");
    assert_eq!(track.meta.name, "Piano");
    assert_eq!(track.note_count, 1);
}

#[test]
fn test_to_midi_document_roundtrip() {
    let doc = make_test_document();
    let project = LuminoProject::from_midi_document(&doc);
    let rebuilt = project.to_midi_document().expect("重建 MidiDocument 失败");

    assert_eq!(rebuilt.track_count, 1);
    assert_eq!(rebuilt.total_ticks, 480);
    assert_eq!(rebuilt.notes[0].len(), 1);
    assert_eq!(rebuilt.tempo_changes.len(), 1);
    assert_eq!(rebuilt.control_events.len(), 1);
    assert_eq!(rebuilt.track_names[0], Some("Piano".into()));
}

/// 编辑后保存回归：UI 编辑 tempo/拍号经统一入口（set_tempo_points /
/// set_time_signatures）同步 document 后，保存链路 from_midi_document
/// 必须读到编辑后的值，而非加载时的原始值（阶段 0/1 修复的
/// "改 BPM/拍号 → 保存丢失回默认 120/4-4" bug 回归测试）。
#[test]
fn test_from_midi_document_after_global_events_edited() {
    let mut doc = make_test_document();
    // 模拟编辑器同步后的 document 权威值
    doc.tempo_changes = vec![(0, 150.0), (1920, 90.5)];
    doc.time_signatures = vec![(0, 4, 4), (1920, 3, 4)];

    let project = LuminoProject::from_midi_document(&doc);

    assert_eq!(project.tempo_changes, vec![(0, 150.0), (1920, 90.5)]);
    assert_eq!(project.time_signatures, vec![(0, 4, 4), (1920, 3, 4)]);
}

/// ProgramChange 保存回归：document.control_events 中的 ProgramChange 事件
/// 必须被 from_midi_document 拆分为 project.program_changes，保证
/// 导入含音色变换的 MIDI 后保存工程不丢失音色数据。
#[test]
fn test_from_midi_document_program_change_preserved() {
    let mut doc = make_test_document();
    doc.control_events = lumino_midi_model::ChunkedList::from_sorted(vec![
        midly::loader::PackedControlEvent::program_change(0, 0, 0, 40),
    ]);

    let project = LuminoProject::from_midi_document(&doc);

    assert_eq!(project.program_changes, vec![(0, 0, 0, 40)]);
}

#[test]
fn test_compact_event_roundtrip() {
    let mut project = LuminoProject::new("Test");
    let events = vec![
        CompactEvent::new(0, 0, EventKind::NoteOn, 0, 60, 100),
        CompactEvent::new(480, 0, EventKind::NoteOff, 0, 60, 0),
    ];
    let data = LmtrackData::from_compact_events(
        TrackMeta {
            track_id: 0,
            name: "Test".into(),
            channel: 0,
            port: 0,
            visibility: TrackVisibilitySer::Visible,
            solo: false,
            is_drum: false,
            max_tick: 480,
        },
        &events,
    );
    project.add_track(data);

    let doc = project.to_midi_document().expect("重建失败");
    assert_eq!(doc.notes[0].len(), 1);
    assert_eq!(doc.notes[0][0].start_tick, 0);
    assert_eq!(doc.notes[0][0].end_tick, 480);
    assert_eq!(doc.notes[0][0].key, 60);
    assert_eq!(doc.notes[0][0].velocity, 100);
}

#[test]
fn test_to_midi_document_roundtrip_overlapping_notes() {
    let doc = MidiDocument { next_note_id: 1,
        notes: vec![lumino_midi_model::ChunkedList::from_sorted(vec![
            NoteEvent::new(0, 480, 60, 100, 0),
            NoteEvent::new(120, 600, 64, 80, 0),
            NoteEvent::new(480, 960, 60, 90, 0),
        ])],
        time_signatures: vec![(0, 4, 4)],
        tempo_changes: vec![(0, 120.0)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_model::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("Piano".into())],
        total_ticks: 960,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![],

        track_max_end_ticks: vec![],
    };
    let project = LuminoProject::from_midi_document(&doc);
    let rebuilt = project.to_midi_document().expect("重叠音符重建失败");

    assert_eq!(rebuilt.notes[0].len(), 3);
    let mut sorted = rebuilt.notes[0].to_vec();
    sorted.sort_by_key(|n| (n.start_tick, n.key));
    assert_eq!(sorted[0].start_tick, 0);
    assert_eq!(sorted[0].end_tick, 480);
    assert_eq!(sorted[0].key, 60);
    assert_eq!(sorted[0].velocity, 100);
    assert_eq!(sorted[1].start_tick, 120);
    assert_eq!(sorted[1].end_tick, 600);
    assert_eq!(sorted[1].key, 64);
    assert_eq!(sorted[1].velocity, 80);
    assert_eq!(sorted[2].start_tick, 480);
    assert_eq!(sorted[2].end_tick, 960);
    assert_eq!(sorted[2].key, 60);
    assert_eq!(sorted[2].velocity, 90);
}

/// 同 key 重叠音符：FIFO 配对必须保持各自长度（旧 HashMap 单槽会覆盖，
/// 导致 A 丢失、B 被截短——"部分变短部分变长"的根因回归测试）
#[test]
fn test_to_midi_document_same_key_overlapping_notes() {
    let mut project = LuminoProject::new("Overlap");
    let events = vec![
        CompactEvent::new(0, 0, EventKind::NoteOn, 0, 60, 100), // A on @0
        CompactEvent::new(120, 0, EventKind::NoteOn, 0, 60, 80), // B on @120
        CompactEvent::new(360, 0, EventKind::NoteOff, 0, 60, 0), // A off @480
        CompactEvent::new(120, 0, EventKind::NoteOff, 0, 60, 0), // B off @600
    ];
    let data = LmtrackData::from_compact_events(
        TrackMeta {
            track_id: 0,
            name: "Overlap".into(),
            channel: 0,
            port: 0,
            visibility: TrackVisibilitySer::Visible,
            solo: false,
            is_drum: false,
            max_tick: 600,
        },
        &events,
    );
    project.add_track(data);

    let doc = project.to_midi_document().expect("同 key 重叠音符重建失败");
    let notes = doc.track_notes(0);
    assert_eq!(notes.len(), 2, "两个重叠音符都应保留");
    let mut sorted = notes.to_vec();
    sorted.sort_by_key(|n| n.start_tick);
    // FIFO：NoteOff 匹配最早的 NoteOn → A=0..480, B=120..600（长度均无损）
    assert_eq!(sorted[0].start_tick, 0);
    assert_eq!(sorted[0].end_tick, 480);
    assert_eq!(sorted[0].key, 60);
    assert_eq!(sorted[1].start_tick, 120);
    assert_eq!(sorted[1].end_tick, 600);
    assert_eq!(sorted[1].key, 60);
}

/// 同 key 首尾相接音符：from_midi_document 稳定排序 + FIFO 回读
/// （sort_unstable 会打乱同 tick 的 NoteOff/NoteOn 顺序 → 长度错乱）
#[test]
fn test_from_midi_document_same_key_adjacent_notes() {
    let doc = MidiDocument { next_note_id: 1,
        notes: vec![lumino_midi_model::ChunkedList::from_sorted(vec![
            NoteEvent::new(0, 480, 60, 100, 0),
            NoteEvent::new(480, 960, 60, 90, 0),
        ])],
        time_signatures: vec![(0, 4, 4)],
        tempo_changes: vec![(0, 120.0)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_model::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("Piano".into())],
        total_ticks: 960,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![],

        track_max_end_ticks: vec![],
    };
    let project = LuminoProject::from_midi_document(&doc);
    let rebuilt = project.to_midi_document().expect("相接音符重建失败");

    assert_eq!(rebuilt.notes[0].len(), 2);
    let mut sorted = rebuilt.notes[0].to_vec();
    sorted.sort_by_key(|n| n.start_tick);
    assert_eq!(sorted[0].start_tick, 0);
    assert_eq!(sorted[0].end_tick, 480);
    assert_eq!(sorted[1].start_tick, 480);
    assert_eq!(sorted[1].end_tick, 960);
}

/// 空白音轨回归：无音符的音轨必须被保存与加载（不再因 track_notes 为空被跳过）。
/// 同时验证隐藏/静音等音轨状态在保存→加载往返后保留。
#[test]
fn test_blank_track_preserved_roundtrip() {
    let doc = MidiDocument {
        next_note_id: 1,
        notes: vec![
            // track 0：有音符
            lumino_midi_model::ChunkedList::from_sorted(vec![NoteEvent::new(0, 480, 60, 100, 0)]),
            // track 1：空白
            lumino_midi_model::ChunkedList::new(),
            // track 2：空白
            lumino_midi_model::ChunkedList::new(),
        ],
        time_signatures: vec![(0, 4, 4)],
        tempo_changes: vec![(0, 120.0)],
        key_signatures: vec![(0, 0, false)],
        control_events: lumino_midi_model::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![
            Some("Melody".into()),
            Some("Harmony".into()),
            Some("Drums".into()),
        ],
        total_ticks: 480,
        track_count: 3,
        tracks: {
            let mut mgr = TrackManager::new(3);
            mgr.set_visibility(1, lumino_midi_model::TrackVisibility::Hidden);
            mgr.set_visibility(2, lumino_midi_model::TrackVisibility::Muted);
            mgr.set_solo(2, true);
            mgr
        },
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: vec![],
    };

    // from_midi_document：3 条音轨都应保留（含 2 条空白）
    let project = LuminoProject::from_midi_document(&doc);
    assert_eq!(project.tracks.len(), 3, "空白音轨不应被丢弃");
    assert_eq!(project.loaded_track_count(), 3);
    assert_eq!(project.get_track(0).unwrap().note_count, 1);
    assert_eq!(project.get_track(1).unwrap().note_count, 0);
    assert_eq!(project.get_track(2).unwrap().note_count, 0);

    // 音轨状态（可见性 / solo）应被保留
    assert_eq!(
        project.get_track(1).unwrap().meta.visibility,
        TrackVisibilitySer::Hidden
    );
    assert_eq!(
        project.get_track(2).unwrap().meta.visibility,
        TrackVisibilitySer::Muted
    );
    assert!(project.get_track(2).unwrap().meta.solo);

    // to_midi_document：往返后音轨数量、名称与状态保留
    let rebuilt = project.to_midi_document().expect("重建失败");
    assert_eq!(rebuilt.track_count, 3);
    assert_eq!(rebuilt.notes[0].len(), 1);
    assert_eq!(rebuilt.notes[1].len(), 0);
    assert_eq!(rebuilt.notes[2].len(), 0);
    assert_eq!(
        rebuilt.track_names,
        vec![
            Some("Melody".into()),
            Some("Harmony".into()),
            Some("Drums".into()),
        ]
    );
    assert_eq!(
        rebuilt.tracks.get(1).unwrap().visibility,
        lumino_midi_model::TrackVisibility::Hidden
    );
    assert_eq!(
        rebuilt.tracks.get(2).unwrap().visibility,
        lumino_midi_model::TrackVisibility::Muted
    );
    assert!(rebuilt.tracks.get(2).unwrap().solo);

    // save → load 往返：空白音效实际写入磁盘并可重新加载
    use crate::project::{load, save};
    let dir = std::env::temp_dir().join("lumino_blank_track_roundtrip");
    let _ = std::fs::remove_dir_all(&dir);
    save::save_to_folder(&project, &dir).expect("保存失败");
    let loaded = load::load_project(&dir).expect("加载失败");
    assert_eq!(loaded.tracks.len(), 3, "加载后空白音轨应保留");
    assert_eq!(
        loaded.get_track(1).unwrap().meta.visibility,
        TrackVisibilitySer::Hidden
    );
    assert_eq!(
        loaded.get_track(2).unwrap().meta.visibility,
        TrackVisibilitySer::Muted
    );
    assert!(loaded.get_track(2).unwrap().meta.solo);
    let _ = std::fs::remove_dir_all(&dir);
}

/// 全空白工程（如新建文件的 Conductor + Setup 两轨均无音符）必须有 track_count 条
/// 音轨被保存/加载，而非坍缩为 0 轨。
#[test]
fn test_all_blank_project_preserved() {
    let doc = MidiDocument {
        next_note_id: 1,
        notes: vec![
            lumino_midi_model::ChunkedList::new(),
            lumino_midi_model::ChunkedList::new(),
        ],
        time_signatures: vec![],
        tempo_changes: vec![(0, 120.0)],
        key_signatures: vec![],
        control_events: lumino_midi_model::ChunkedList::new(),
        lyrics: vec![],
        markers: vec![],
        sys_ex: vec![],
        track_names: vec![Some("Conductor".into()), Some("Setup".into())],
        total_ticks: 0,
        track_count: 2,
        tracks: TrackManager::new(2),
        division: 480,
        track_ports: vec![],
        track_max_end_ticks: vec![],
    };

    let project = LuminoProject::from_midi_document(&doc);
    assert_eq!(project.tracks.len(), 2, "全空白工程应保留 2 条音轨");
    assert_eq!(project.metadata.audio.track_count, 2);

    let rebuilt = project.to_midi_document().expect("重建失败");
    assert_eq!(rebuilt.track_count, 2);
    assert_eq!(rebuilt.notes.len(), 2);
    assert_eq!(rebuilt.notes[0].len(), 0);
    assert_eq!(rebuilt.notes[1].len(), 0);
}
