//! `metadata.toml` 结构往返契约（从 `metadata.rs` 内联测试迁出）。
//!
//! 拆分原因：`metadata.rs` 已超过 400 行红线；这里只用公开 API，
//! 正好作为工程文件格式的外部契约测试。

use lumino_project::project::metadata::MaterialMetadata;
use lumino_project::{ProjectMetadata, SettingsMetadata};

#[test]
fn test_metadata_roundtrip() {
    use lumino_project::project::metadata::TrackMetadataEntry;

    let mut meta = ProjectMetadata::default_with_name("Test Project");
    meta.audio.track_count = 2;
    meta.audio.total_notes = 1000;
    meta.tracks.entries.push(TrackMetadataEntry {
        track_id: 0,
        name: "Piano".into(),
        channel: 0,
        visibility: "visible".into(),
        solo: false,
        note_count: 500,
    });

    let toml_str = meta.to_toml_str().expect("序列化元数据为TOML失败");
    let decoded = ProjectMetadata::from_toml_str(&toml_str).expect("从TOML反序列化元数据失败");

    assert_eq!(decoded.project.name, "Test Project");
    assert_eq!(decoded.audio.track_count, 2);
    assert_eq!(decoded.tracks.entries.len(), 1);
    assert_eq!(decoded.tracks.entries[0].name, "Piano");
}

#[test]
fn test_default_metadata() {
    let meta = ProjectMetadata::default_with_name("Untitled");
    assert_eq!(meta.format_version, 1);
    assert_eq!(meta.project.name, "Untitled");
    assert_eq!(meta.audio.division, 480);
    assert!(!meta.is_material_file());
    assert_eq!(meta.material_track_count(), 0);
}

#[test]
fn test_material_metadata_roundtrip() {
    let mut meta = ProjectMetadata::default_with_name("My Material");
    meta.material = Some(MaterialMetadata::for_track_count(4));

    let toml_str = meta.to_toml_str().expect("序列化素材元数据失败");
    let decoded = ProjectMetadata::from_toml_str(&toml_str).expect("反序列化素材元数据失败");

    assert!(decoded.is_material_file());
    assert!(matches!(decoded.material, Some(ref m) if m.multi_track && m.track_count == Some(4)));
    assert_eq!(decoded.material_track_count(), 4);
}

#[test]
fn test_single_track_material_omits_track_count() {
    let meta = MaterialMetadata::for_track_count(1);
    assert!(meta.is_material);
    assert!(!meta.multi_track);
    assert!(meta.track_count.is_none());

    let mut project = ProjectMetadata::default_with_name("Single");
    project.material = Some(meta);
    assert_eq!(project.material_track_count(), 1);
    // 单轨素材的 [material] 段不序列化 track_count 字段
    // （audio.track_count 是工程统计字段，始终存在，需截取 [material] 段后断言）
    let toml_str = project.to_toml_str().expect("序列化失败");
    let material_section = toml_str
        .split("[material]")
        .nth(1)
        .expect("应包含 [material] 段");
    assert!(!material_section.contains("track_count"));
}

#[test]
fn test_i2m_settings_roundtrip() {
    // DRAW-030：图片转 MIDI 参数随工程落盘，重开恢复
    let mut meta = ProjectMetadata::default_with_name("I2M");
    let i2m = lumino_core::I2mSettings {
        start_key: 36,
        color_count: 8,
        brightness: 15,
        palette: "octree".into(),
        blend: "spread".into(),
        ..lumino_core::I2mSettings::default()
    };
    meta.settings = Some(SettingsMetadata {
        i2m: Some(i2m.clone()),
        ..Default::default()
    });

    let toml_str = meta.to_toml_str().expect("序列化失败");
    let decoded = ProjectMetadata::from_toml_str(&toml_str).expect("反序列化失败");
    let restored = decoded
        .settings
        .and_then(|settings| settings.i2m)
        .expect("应包含 i2m 段");
    assert_eq!(restored, i2m);
}

#[test]
fn test_legacy_metadata_without_material_parses() {
    // 上一代工程文件没有 material 段，必须能正常解析（向后兼容）
    let legacy = r#"
format_version = 1

[project]
name = "Legacy"
author = "Anonymous"
created_at = "2026-01-01T00:00:00+00:00"
modified_at = "2026-01-01T00:00:00+00:00"
lumino_version = "0.1.0"

[audio]
division = 480
total_ticks = 1000
track_count = 1
total_notes = 10
default_bpm = 120.0

[tracks]
entries = []
"#;
    let decoded = ProjectMetadata::from_toml_str(legacy).expect("旧版元数据解析失败");
    assert!(!decoded.is_material_file());
    assert!(decoded.material.is_none());
    assert_eq!(decoded.project.name, "Legacy");
}
