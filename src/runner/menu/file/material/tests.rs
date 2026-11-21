//! 素材工程构建（.lmmaterial）单测：时间轴归一化与跨轨槽位压缩。

use super::*;

use crate::runner::menu::file::editor_midi::EditorNotes;

/// 构造带"引子空白"的文档：第一个音符在 tick 1000（素材不应包含引子空白）
fn make_doc_with_leading_gap() -> MidiDocument {
    use lumino_midi_loader::{ChunkedList, NoteEvent, TrackManager};

    MidiDocument {
        notes: vec![ChunkedList::from_sorted(vec![
            NoteEvent::new(1000, 1480, 60, 100, 0),
            NoteEvent::new(2000, 2240, 62, 90, 0),
        ])],
        tempo_changes: vec![(0, 90.0)],
        time_signatures: vec![(0, 2, 4), (1000, 4, 4)],
        key_signatures: vec![],
        control_events: ChunkedList::from_sorted(vec![
            midly::loader::PackedControlEvent::control_change(500, 0, 0, 10, 64),
            midly::loader::PackedControlEvent::control_change(1200, 0, 0, 10, 80),
        ]),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![Some("Piano".to_string())],
        total_ticks: 3000,
        track_count: 1,
        tracks: TrackManager::new(1),
        division: 480,
        track_ports: vec![0],
        track_max_end_ticks: MidiDocument::new_track_max_ticks(1),
    }
}

/// 素材时间轴归一化：开头对齐第一个音符，长度由最后一个音符决定
#[test]
fn test_build_material_normalizes_to_first_note() {
    use lumino_midi_loader::{CompactEvent, EventKind};

    let doc = make_doc_with_leading_gap();
    // 框选仅覆盖音符本身（与走带框选相同语义：只取选中音符）
    let selected: EditorNotes = vec![(
        0,
        vec![(1000.0, 60, 480.0, 100, 0), (2000.0, 62, 240.0, 90, 0)],
    )];
    // 速度点来自实测文档（make_doc_with_leading_gap 里 tempo_changes = [(0, 90.0)]）
    let project = build_material_project_from_selection(&doc, &selected);

    // 素材长度 = 最后一个音符结束(2240) - 第一个音符开始(1000)，不跟随框选/引子空白
    assert_eq!(project.metadata.audio.total_ticks, 1240);
    assert_eq!(project.metadata.audio.total_notes, 2);
    // 片段前的拍号（2/4 引子）丢弃，片段内拍号平移到 0
    assert_eq!(project.time_signatures, vec![(0, 4, 4)]);
    // 片段前的 tempo 收敛到 0 保留（速度信息不可丢）
    assert_eq!(project.tempo_changes, vec![(0, 90.0)]);
    // 片段前的 CC 丢弃，片段内的平移到素材时间轴
    assert_eq!(project.control_changes, vec![(200, 0, 0, 10, 80)]);

    // 音符事件：第一个音符从 0 开始，第二个音符结束于 total_ticks
    let track = project.get_track(0).expect("应包含音轨 0");
    let events: Vec<CompactEvent> = track.compact_events().expect("解码音轨失败").collect();
    let mut tick = 0_u32;
    let mut note_starts = Vec::new();
    let mut note_ends = Vec::new();
    for ev in events {
        tick = tick.saturating_add(ev.delta_tick());
        if ev.kind() == EventKind::NoteOn {
            note_starts.push(tick);
        } else if ev.kind() == EventKind::NoteOff {
            note_ends.push(tick);
        }
    }
    assert_eq!(note_starts, vec![0, 1000]);
    assert_eq!(note_ends, vec![480, 1240]);
}

/// 素材开头无空白（第一个音符在 tick 0）时保持原样
#[test]
fn test_build_material_keeps_original_when_start_at_zero() {
    use lumino_midi_loader::{ChunkedList, NoteEvent, TrackManager};

    let doc = MidiDocument {
        notes: vec![ChunkedList::from_sorted(vec![NoteEvent::new(
            0, 480, 60, 100, 0,
        )])],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
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
        track_ports: vec![0],
        track_max_end_ticks: MidiDocument::new_track_max_ticks(1),
    };
    let selected: EditorNotes = vec![(0, vec![(0.0, 60, 480.0, 100, 0)])];

    let project = build_material_project_from_selection(&doc, &selected);
    assert_eq!(project.metadata.audio.total_ticks, 480);
    assert_eq!(project.time_signatures, vec![(0, 4, 4)]);
    assert_eq!(project.tempo_changes, vec![(0, 120.0)]);
}

/// 三轨文档：音符在轨 1、2（轨 0 不在选区），用于验证"非 0 起"的跨轨选区
fn make_three_track_doc() -> MidiDocument {
    use lumino_midi_loader::{ChunkedList, NoteEvent, TrackManager};

    MidiDocument {
        notes: vec![
            // 轨 0：不在选区（若按绝对轨号落盘，会变成素材里的空槽）
            ChunkedList::from_sorted(vec![NoteEvent::new(0, 240, 48, 80, 2)]),
            ChunkedList::from_sorted(vec![NoteEvent::new(1000, 1480, 60, 100, 0)]),
            ChunkedList::from_sorted(vec![NoteEvent::new(1200, 1620, 40, 90, 1)]),
        ],
        tempo_changes: vec![(0, 120.0)],
        time_signatures: vec![(0, 4, 4)],
        key_signatures: vec![],
        control_events: ChunkedList::from_sorted(vec![
            // 选中轨 1 的 CC
            midly::loader::PackedControlEvent::control_change(1100, 1, 0, 10, 64),
            // 未选中轨 0 的 CC（必须丢弃）
            midly::loader::PackedControlEvent::control_change(600, 0, 0, 7, 99),
            // 选中轨 2 的 PC
            midly::loader::PackedControlEvent::program_change(1300, 2, 1, 5),
        ]),
        lyrics: vec![],
        markers: vec![],
        text_events: vec![],
        sys_ex: vec![],
        track_names: vec![
            Some("Lead".to_string()),
            Some("Piano".to_string()),
            Some("Bass".to_string()),
        ],
        total_ticks: 2000,
        track_count: 3,
        tracks: TrackManager::new(3),
        division: 480,
        track_ports: vec![0, 0, 0],
        track_max_end_ticks: MidiDocument::new_track_max_ticks(3),
    }
}

/// 跨轨素材（源轨 1、2）必须压缩为素材内 0、1 连续槽位：
/// 归档按槽位号落盘、按 metadata.track_count 从 0 连续读取，沿用绝对轨号
/// 会在加载后多出空轨、音符错位（真机表现为"导出后没有产物"）。
#[test]
fn test_build_material_compacts_multi_track_slots() {
    let doc = make_three_track_doc();
    let selected: EditorNotes = vec![
        (1, vec![(1000.0, 60, 480.0, 100, 0)]),
        (2, vec![(1200.0, 40, 420.0, 90, 1)]),
    ];
    let project = build_material_project_from_selection(&doc, &selected);

    // 素材内槽位从 0 连续（源轨 1/2 → 素材轨 0/1），不存在空槽
    assert_eq!(
        project.tracks.len(),
        2,
        "素材工程不得保留源工程绝对轨索引空洞"
    );
    assert_eq!(project.metadata.audio.track_count, 2);
    assert_eq!(
        project.track_names,
        vec![Some("Piano".to_string()), Some("Bass".to_string())],
        "轨道名必须与素材槽位同序（不能整份文档轨名照抄）"
    );
    // 控制事件随槽位换算：轨 1 → 0，轨 2 → 1；未选中的轨 0 事件被丢弃
    assert_eq!(
        project
            .control_changes
            .iter()
            .map(|&(_, track, ..)| track)
            .collect::<Vec<_>>(),
        vec![0]
    );
    assert_eq!(
        project
            .program_changes
            .iter()
            .map(|&(_, track, ..)| track)
            .collect::<Vec<_>>(),
        vec![1]
    );

    // 落盘 → 重新加载：素材轨数、每轨音符与选区一致（本条覆盖双轨 round-trip）
    let dir = std::env::temp_dir().join(format!(
        "lumino_material_multitrack_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("临时目录应创建成功");
    let path = dir.join("multi_track.lmmaterial");
    lumino_export::save_material(&project, "MultiTrack", &path).expect("保存素材失败");

    let loaded = lumino_export::load_project(&path).expect("加载素材失败");
    assert_eq!(
        loaded.metadata.audio.track_count, 2,
        "归档 metadata 的轨数必须等于素材实际槽位数"
    );
    assert_eq!(loaded.metadata.material_track_count(), 2);
    assert_eq!(
        loaded.loaded_track_count(),
        2,
        "归档不得含空槽（否则加载后前面多出空轨）"
    );

    let parsed =
        lumino_export::project_to_parsed_midi(&loaded, &path).expect("重建 ParsedMidi 失败");
    let loaded_doc = parsed.document.expect("应包含 MidiDocument");
    assert_eq!(
        loaded_doc.track_count(),
        2,
        "重新加载应为两轨，而非多出空轨"
    );
    assert_eq!(loaded_doc.track_notes(0).len(), 1);
    assert_eq!(loaded_doc.track_notes(1).len(), 1);
    let first = loaded_doc
        .track_notes(0)
        .iter()
        .next()
        .copied()
        .expect("素材轨 0 应包含音符");
    assert_eq!((first.key, first.channel), (60, 0));
    assert_eq!(first.length(), 480, "音符长度不得因导出/加载改变");
    let second = loaded_doc
        .track_notes(1)
        .iter()
        .next()
        .copied()
        .expect("素材轨 1 应包含音符");
    assert_eq!((second.key, second.channel), (40, 1));
    assert_eq!(second.length(), 420, "音符长度不得因导出/加载改变");

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir_all(&dir);
}
