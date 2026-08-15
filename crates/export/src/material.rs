//! 素材文件（.lmmaterial）保存与判断
//!
//! 素材 = 带 `[material]` 元数据标记的单文件工程归档（复用 LMPJ 归档格式），
//! 与标准 `.lmpj` 的唯一区别是 metadata 中携带 `MaterialMetadata`。

use std::path::Path;

use lumino_project::LuminoProject;
use lumino_project::project::metadata::MaterialMetadata;

use crate::ExportResult;

/// 保存为素材文件（.lmmaterial）
///
/// - 素材名写入 `metadata.project.name`（二次导出以新名字为准）；
/// - 按已加载音轨数量自动推导单/多轨形态并写入 `MaterialMetadata`；
/// - 自动化等工程级数据（tempo/CC/PC/弯音/SysEx 等）随归档完整保留。
pub fn save_material(
    project: &LuminoProject,
    material_name: &str,
    path: impl AsRef<Path>,
) -> ExportResult<()> {
    let mut project = project.clone();
    // 素材名：以本次导出设置的名字为准
    project.metadata.project.name = material_name.to_string();
    project.metadata.material = Some(MaterialMetadata::for_track_count(
        project.loaded_track_count(),
    ));
    crate::project::save_to_archive(&project, path).map_err(crate::ExportError::from)
}

/// 判断路径是否为素材文件（.lmmaterial）
pub fn is_material_path(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("lmmaterial"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{load_project, project_to_parsed_midi};
    use lumino_midi_model::compact::{CompactEvent, EventKind};
    use lumino_project::{
        LmtrackData, TrackMeta, TrackVisibilitySer, project::save::save_to_archive,
    };
    use tempfile::tempdir;

    /// 单轨测试工程
    fn make_test_project() -> LuminoProject {
        let mut project = LuminoProject::new("FolderEntryTest");
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
        project.metadata.audio.total_ticks = 480;
        project
    }

    /// 双轨测试工程（验证多轨素材）
    fn make_two_track_project() -> LuminoProject {
        let mut project = make_test_project();
        let track1 = LmtrackData::from_compact_events(
            TrackMeta {
                track_id: 1,
                name: "Bass".into(),
                channel: 1,
                port: 0,
                visibility: TrackVisibilitySer::Visible,
                solo: false,
                is_drum: false,
                max_tick: 480,
            },
            &[
                CompactEvent::new(0, 1, EventKind::NoteOn, 1, 40, 90),
                CompactEvent::new(480, 1, EventKind::NoteOff, 1, 40, 0),
            ],
        );
        project.add_track(track1);
        project.metadata.audio.total_ticks = 480;
        project
    }

    #[test]
    fn test_save_and_load_material_roundtrip() {
        let dir = tempdir().expect("临时目录应创建成功");
        let material_path = dir.path().join("my_material.lmmaterial");
        let project = make_two_track_project();

        save_material(&project, "MyMaterial", &material_path).expect("保存素材失败");
        assert!(is_material_path(&material_path));

        // 加载后通过 metadata 分辨素材文件
        let loaded = load_project(&material_path).expect("加载素材失败");
        assert!(loaded.metadata.is_material_file());
        assert_eq!(loaded.metadata.material_track_count(), 2);
        // 素材名以导出时设置的名字为准
        assert_eq!(loaded.metadata.project.name, "MyMaterial");
        // 音轨数据完整（拖放到卷帘的基础）
        assert_eq!(loaded.loaded_track_count(), 2);

        // 素材可重建为 ParsedMidi（卷帘预览/写入的数据源）
        let parsed = project_to_parsed_midi(&loaded, &material_path).expect("重建 ParsedMidi 失败");
        let document = parsed.document.expect("应包含 MidiDocument");
        assert_eq!(document.track_count(), 2);
        assert_eq!(document.notes[0].len(), 1);
        assert_eq!(document.notes[1].len(), 1);
    }

    #[test]
    fn test_save_material_single_track_marks_single() {
        let dir = tempdir().expect("临时目录应创建成功");
        let material_path = dir.path().join("single.lmmaterial");
        let project = make_test_project(); // 单轨

        save_material(&project, "Single", &material_path).expect("保存素材失败");
        let loaded = load_project(&material_path).expect("加载素材失败");
        assert!(loaded.metadata.is_material_file());
        assert!(!matches!(loaded.metadata.material, Some(ref m) if m.multi_track));
        assert_eq!(loaded.metadata.material_track_count(), 1);
    }

    #[test]
    fn test_material_renamed_lmpj_still_detected_by_metadata() {
        // 即使把 .lmmaterial 重命名为 .lmpj，也应通过 metadata 识别为素材
        let dir = tempdir().expect("临时目录应创建成功");
        let src = dir.path().join("mat.lmmaterial");
        save_material(&make_test_project(), "Mat", &src).expect("保存素材失败");

        let renamed = dir.path().join("mat.lmpj");
        std::fs::copy(&src, &renamed).expect("复制素材失败");

        let loaded = load_project(&renamed).expect("加载重命名素材失败");
        assert!(loaded.metadata.is_material_file());
    }

    #[test]
    fn test_save_material_keeps_archive_compatible() {
        // 素材归档与标准 lmpj 归档同格式（LMPJ 魔数 + bincode），可被普通工程加载器解析
        let dir = tempdir().expect("临时目录应创建成功");
        let material_path = dir.path().join("compat.lmmaterial");
        save_material(&make_test_project(), "Compat", &material_path).expect("保存素材失败");

        let bytes = std::fs::read(&material_path).expect("读取素材失败");
        assert!(bytes.len() >= 4 && &bytes[0..4] == b"LMPJ");

        // 同一字节流也可走内存加载（嵌入式素材的解析路径）
        let loaded =
            lumino_project::project::load::load_project_from_bytes(&bytes).expect("内存加载失败");
        assert!(loaded.metadata.is_material_file());
    }

    #[test]
    fn test_save_material_keeps_author_from_metadata() {
        // 素材署名链路：工程设置面板的作者 → metadata.project.author → 素材库悬浮窗
        // 导出素材时作者必须原样保留（runner 在导出前从工程设置对话框写入）
        let dir = tempdir().expect("临时目录应创建成功");
        let material_path = dir.path().join("authored.lmmaterial");
        let mut project = make_test_project();
        project.metadata.project.author = "Lumino 用户".into();

        save_material(&project, "Authored", &material_path).expect("保存素材失败");
        let loaded = load_project(&material_path).expect("加载素材失败");
        assert_eq!(loaded.metadata.project.author, "Lumino 用户");
    }

    #[test]
    fn test_archive_save_still_works() {
        // 回归：save_to_archive 不受素材改造影响
        let dir = tempdir().expect("临时目录应创建成功");
        let path = dir.path().join("normal.lmpj");
        save_to_archive(&make_test_project(), &path).expect("保存归档失败");
        let loaded = load_project(&path).expect("加载归档失败");
        assert!(!loaded.metadata.is_material_file());
    }
}
