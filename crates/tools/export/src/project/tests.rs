use super::*;
use lumino_midi_model::compact::{CompactEvent, EventKind};
use lumino_project::project::track::{LmtrackData, TrackMeta, TrackVisibilitySer};
use tempfile::tempdir;

fn make_test_project() -> LuminoProject {
    let mut project = LuminoProject::new("FolderEntryTest");
    let events = vec![
        CompactEvent::new(0, 0, EventKind::NoteOn, 0, 60, 100),
        CompactEvent::new(480, 0, EventKind::NoteOff, 0, 60, 0),
    ];
    let track = LmtrackData::from_compact_events(
        TrackMeta {
            track_id: 0,
            name: "Piano".into(),
            channel: 0,
            port: 0,
            visibility: TrackVisibilitySer::Visible,
            solo: false,
            is_drum: false,
            max_tick: 480,
        },
        &events,
    );
    project.add_track(track);
    project.metadata.audio.total_ticks = 480;
    project.metadata.audio.division = 480;
    project
}

#[test]
fn test_save_and_load_folder_entry() {
    let dir = tempdir().expect("临时目录应创建成功");
    let entry_path = dir.path().join("test_project.lmpj");
    let project = make_test_project();

    save_project_to_folder_with_entry(&project, &entry_path, 128).expect("保存文件夹工程入口失败");

    // 入口文件与数据文件夹应同时存在
    assert!(entry_path.exists(), "入口文件应存在");
    let data_folder = entry_path.with_extension("");
    assert!(data_folder.is_dir(), "数据文件夹应存在");
    assert!(data_folder.join("metadata.toml").exists());
    assert!(data_folder.join("data/project/tracks/000.lmtrack").exists());

    // 加载入口文件应得到相同工程
    let loaded = load_project(&entry_path).expect("加载入口文件失败");
    assert_eq!(loaded.metadata.project.name, "FolderEntryTest");
    assert_eq!(loaded.tracks.len(), 1);
    assert_eq!(loaded.metadata.audio.total_ticks, 480);
}

#[test]
fn test_save_and_load_folder_entry_multi_track_overlapping() {
    let dir = tempdir().expect("临时目录应创建成功");
    let entry_path = dir.path().join("multi_project.lmpj");
    let mut project = LuminoProject::new("MultiTrackOverlap");

    // 音轨 0：单音符
    let track0 = LmtrackData::from_compact_events(
        TrackMeta {
            track_id: 0,
            name: "Piano".into(),
            channel: 0,
            port: 0,
            visibility: TrackVisibilitySer::Visible,
            solo: false,
            is_drum: false,
            max_tick: 480,
        },
        &[
            CompactEvent::new(0, 0, EventKind::NoteOn, 0, 60, 100),
            CompactEvent::new(480, 0, EventKind::NoteOff, 0, 60, 0),
        ],
    );
    project.add_track(track0);

    // 音轨 1：重叠音符（曾触发 to_midi_document 的交替 NoteOn/NoteOff 假设）
    let track1 = LmtrackData::from_compact_events(
        TrackMeta {
            track_id: 1,
            name: "Synth".into(),
            channel: 1,
            port: 0,
            visibility: TrackVisibilitySer::Visible,
            solo: false,
            is_drum: false,
            max_tick: 600,
        },
        &[
            // 两个音符重叠：0-480 与 120-600
            CompactEvent::new(0, 1, EventKind::NoteOn, 1, 64, 100),
            CompactEvent::new(120, 1, EventKind::NoteOn, 1, 67, 80),
            CompactEvent::new(360, 1, EventKind::NoteOff, 1, 64, 0),
            CompactEvent::new(120, 1, EventKind::NoteOff, 1, 67, 0),
        ],
    );
    project.add_track(track1);
    project.metadata.audio.total_ticks = 600;
    project.metadata.audio.division = 480;

    save_project_to_folder_with_entry(&project, &entry_path, 128)
        .expect("保存多轨文件夹工程入口失败");

    // 加载后通过 project_to_parsed_midi 重建，这是 Runner 的加载路径
    let loaded = load_project(&entry_path).expect("加载多轨入口文件失败");
    let parsed = project_to_parsed_midi(&loaded, &entry_path).expect("重建 ParsedMidi 失败");

    let document = parsed.document.expect("应包含 MidiDocument");
    assert_eq!(document.track_count(), 2);
    assert_eq!(document.notes[0].len(), 1);
    assert_eq!(document.notes[1].len(), 2);

    // 验证重叠音符被正确重建（ChunkedList 已保证有序，无需再排序）
    let track1_notes = &document.notes[1];
    assert_eq!(track1_notes[0].start_tick, 0);
    assert_eq!(track1_notes[0].end_tick, 480);
    assert_eq!(track1_notes[0].key, 64);
    assert_eq!(track1_notes[1].start_tick, 120);
    assert_eq!(track1_notes[1].end_tick, 600);
    assert_eq!(track1_notes[1].key, 67);
}

#[test]
fn test_working_time_survives_save_load_roundtrip() {
    let dir = tempdir().expect("临时目录应创建成功");
    let mut project = make_test_project();
    project.set_working_time_seconds(12345.6);

    // 归档形态：保存 → 加载 → 累计时间保留（四舍五入取整）
    let archive_path = dir.path().join("stats_project.lmpj");
    save_to_archive(&project, &archive_path).expect("保存归档失败");
    let loaded = load_project(&archive_path).expect("加载归档失败");
    assert_eq!(loaded.working_time_seconds(), 12346.0);

    // Runner 的加载路径：project_to_parsed_midi 必须把累计时间传给 ParsedMidi
    let parsed = project_to_parsed_midi(&loaded, &archive_path).expect("重建 ParsedMidi 失败");
    assert_eq!(parsed.accumulated_editing_secs, 12346.0);

    // 文件夹形态同样保留（入口路径需带扩展名：内部 with_extension("") 推导数据目录，
    // 无扩展名路径在 Windows 上会生成带尾点的非法目录名）
    let folder_entry = dir.path().join("stats_folder.lmpj");
    save_project_to_folder_with_entry(&project, &folder_entry, 128).expect("保存文件夹工程失败");
    let loaded2 = load_project(&folder_entry).expect("加载文件夹工程失败");
    assert_eq!(loaded2.working_time_seconds(), 12346.0);
}

#[test]
fn test_load_project_image_metadata() {
    let dir = tempdir().expect("临时目录应创建成功");
    let entry_path = dir.path().join("img_project.lmpj");
    let project = make_test_project();

    save_project_to_folder_with_entry(&project, &entry_path, 128).expect("保存文件夹工程入口失败");

    let meta = load_project_image_metadata(&entry_path).expect("应能读取到 image 元数据");
    assert!(!meta.cache_hash.is_empty());
    assert_eq!(meta.key_count, 128);
    assert_eq!(meta.measures_per_group, 4);
    assert_eq!(meta.tile_width_px, 1920);
}

#[test]
fn test_load_legacy_and_archive_not_image_metadata() {
    let dir = tempdir().expect("临时目录应创建成功");
    let non_entry = dir.path().join("not_entry.lmpj");
    std::fs::write(&non_entry, b"LMPJ\x00\x01").expect("写入测试文件失败");
    assert!(load_project_image_metadata(&non_entry).is_none());
}

#[test]
fn test_load_project_from_plain_folder() {
    let dir = tempdir().expect("临时目录应创建成功");
    let data_folder = dir.path().join("plain_folder");
    let project = make_test_project();
    lumino_project::project::save::save_to_folder(&project, &data_folder)
        .expect("保存到文件夹失败");

    let loaded = load_project(&data_folder).expect("加载普通文件夹失败");
    assert_eq!(loaded.metadata.project.name, "FolderEntryTest");
}
