//! 已删除音轨缓存的单元测试

use super::*;
use std::path::PathBuf;

fn sample_meta(track_id: u16, track_name: &str) -> DeletedTrackMetadata {
    DeletedTrackMetadata {
        track_id,
        track_name: track_name.to_string(),
        port: 0,
        channel: 0,
        note_count: 2,
        deleted_at: "ts:1000".to_string(),
        original_index: 0,
        is_drum: false,
        max_tick: 960,
    }
}

fn sample_data() -> DeletedTrackData {
    DeletedTrackData {
        notes: vec![
            DeletedNote {
                start_tick: 0,
                end_tick: 480,
                key: 60,
                velocity: 100,
                channel: 0,
                port: 0,
            },
            DeletedNote {
                start_tick: 480,
                end_tick: 960,
                key: 62,
                velocity: 90,
                channel: 0,
                port: 0,
            },
        ],
    }
}

#[test]
fn test_save_and_load_roundtrip() {
    let temp_dir = std::env::temp_dir().join(format!(
        "lumino_deleted_track_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&temp_dir).expect("创建临时目录失败");

    let meta = sample_meta(1, "TestTrack");
    let data = sample_data();

    let saved_path = save_deleted_track(&temp_dir, &meta, &data).expect("保存失败");
    let (loaded_meta, loaded_data) = load_deleted_track(&saved_path).expect("加载失败");

    assert_eq!(loaded_meta.track_id, meta.track_id);
    assert_eq!(loaded_meta.track_name, meta.track_name);
    assert_eq!(loaded_meta.note_count, meta.note_count);
    assert_eq!(loaded_data.notes.len(), data.notes.len());
    assert_eq!(loaded_data.notes[0].start_tick, data.notes[0].start_tick);

    // 清理
    let _ = std::fs::remove_file(&saved_path);
    let _ = std::fs::remove_dir(&temp_dir);
}

#[test]
fn test_list_deleted_tracks_sorted_by_deleted_at_desc() {
    let temp_dir = std::env::temp_dir().join(format!(
        "lumino_deleted_track_list_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&temp_dir).expect("创建临时目录失败");

    // 保存两个不同时间的缓存
    let mut meta_old = sample_meta(1, "OldTrack");
    meta_old.deleted_at = "ts:100".to_string();
    let _ = save_deleted_track(&temp_dir, &meta_old, &sample_data()).expect("保存旧缓存失败");

    let mut meta_new = sample_meta(2, "NewTrack");
    meta_new.deleted_at = "ts:200".to_string();
    let _ = save_deleted_track(&temp_dir, &meta_new, &sample_data()).expect("保存新缓存失败");

    let entries = list_deleted_tracks(&temp_dir).expect("扫描失败");
    assert_eq!(entries.len(), 2);
    // 倒序排列：新的在前
    assert_eq!(entries[0].metadata.track_id, 2);
    assert_eq!(entries[1].metadata.track_id, 1);

    // 清理
    for entry in &entries {
        let _ = std::fs::remove_file(&entry.path);
    }
    let _ = std::fs::remove_dir(&temp_dir);
}

#[test]
fn test_delete_permanently() {
    let temp_dir = std::env::temp_dir().join(format!(
        "lumino_deleted_track_del_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&temp_dir).expect("创建临时目录失败");

    let path = save_deleted_track(&temp_dir, &sample_meta(1, "ToDelete"), &sample_data())
        .expect("保存失败");
    assert!(path.exists());

    delete_permanently(&path).expect("永久删除失败");
    assert!(!path.exists());

    // 重复删除应静默成功
    delete_permanently(&path).expect("重复删除应静默成功");

    // 清理
    let _ = std::fs::remove_dir(&temp_dir);
}

#[test]
fn test_list_nonexistent_dir_returns_empty() {
    let path = PathBuf::from("/nonexistent/path/that/does/not/exist");
    let entries = list_deleted_tracks(&path).expect("不存在目录应返回空列表");
    assert!(entries.is_empty());
}

#[test]
fn test_build_filename_via_save() {
    // 通过 save 间接验证 build_filename：名称为空时回退到 track_{id}
    let temp_dir = std::env::temp_dir().join(format!(
        "lumino_deleted_track_name_test_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&temp_dir).expect("创建临时目录失败");

    let mut meta = sample_meta(5, "");
    meta.deleted_at = "ts:300".to_string();
    let path = save_deleted_track(&temp_dir, &meta, &sample_data()).expect("保存失败");

    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    assert!(
        filename.starts_with("track_5"),
        "空名称应回退到 track_{{id}}，实际: {filename}"
    );

    // 清理
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_dir(&temp_dir);
}
