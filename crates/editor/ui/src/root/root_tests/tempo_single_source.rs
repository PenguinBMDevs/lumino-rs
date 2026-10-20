//! Tempo / 拍号单一权威源契约测试
//!
//! 审计 §三：`EditorData.tempo_points` / `time_signatures` 与
//! `MidiDocument.tempo_changes` / `time_signatures` 曾是两份需要"手工同步 +
//! 六处出口打补丁"的状态。整改后：
//!
//! 1. **写入**：所有 tempo 写入入口（`set_tempo_points` / `set_tempo_point` /
//!    `add_tempo_point` / `remove_tempo_point`）都经 `sync_tempo_to_document`
//!    写穿到 document，document 是保存/导出的唯一权威；
//! 2. **装载**：`Root::set_midi_document` 只读刷新编辑态缓存
//!    （`adopt_tempo_cache_from_document`），不再"读出 → 再写回"；
//! 3. **出口**：保存 / 工程导出 / 云存 / 音频导出 / 视频导出 / 素材构建
//!    全部直接读 document，不再调用 `apply_tempo_points` 补丁。
//!
//! 本文件把 1 和 3 钉成回归断言——任何一处回退（例如有人"顺手"把补丁加回来
//! 并依赖它）都会在这里暴露。

use lumino_editor_state::EditorData;
use lumino_export::LuminoProject;
use lumino_midi_loader::MidiDocument;
use lumino_note_core::midi_types::TempoPoint;

/// 构造含一份 document（tempo=120、4/4）的编辑器数据
fn data_with_document() -> EditorData {
    let mut doc = MidiDocument::empty_with_tracks(2, 480);
    doc.tempo_changes = vec![(0, 120.0)];
    doc.time_signatures = vec![(0, 4, 4)];

    let mut data = EditorData::new();
    data.document = Some(doc);
    data
}

/// tempo 编辑必须写穿到 document，且出口工程直接读到编辑后的值（无需补丁）
#[test]
fn tempo_edit_reaches_export_project_without_patch() {
    let mut data = data_with_document();

    // 用户在速度面板改 BPM
    assert!(data.set_tempo_point(0, 150.0));

    let doc = data.document.as_ref().expect("文档应存在");
    assert_eq!(
        doc.tempo_changes[0].1, 150.0,
        "写穿：document 必须立即反映编辑（这是出口唯一的读取来源）"
    );

    // 出口直接用 document 构建工程——不调用任何 apply_tempo_points
    let project = LuminoProject::from_midi_document(doc);
    assert!(
        project
            .tempo_changes
            .iter()
            .any(|&(tick, bpm)| tick == 0 && (bpm - 150.0).abs() < 1e-3),
        "工程 tempo 必须是编辑后的 BPM，无需补丁: {:?}",
        project.tempo_changes
    );
}

/// 整体替换 / 增 / 删三条 tempo 写入路径同样写穿
#[test]
fn all_tempo_write_paths_sync_document() {
    let mut data = data_with_document();

    // 整体替换
    data.set_tempo_points(vec![
        TempoPoint {
            tick: 0.0,
            bpm: 100.0,
        },
        TempoPoint {
            tick: 960.0,
            bpm: 60.0,
        },
    ]);
    {
        let doc = data.document.as_ref().expect("文档应存在");
        assert_eq!(doc.tempo_changes, vec![(0, 100.0), (960, 60.0)]);
    }

    // 新增（同 tick 去重、按 tick 排序后写穿）
    data.add_tempo_point(480.0, 80.0);
    {
        let doc = data.document.as_ref().expect("文档应存在");
        assert_eq!(
            doc.tempo_changes,
            vec![(0, 100.0), (480, 80.0), (960, 60.0)]
        );
    }

    // 删除
    assert!(data.remove_tempo_point(1));
    {
        let doc = data.document.as_ref().expect("文档应存在");
        assert_eq!(doc.tempo_changes, vec![(0, 100.0), (960, 60.0)]);
    }
}

/// 拍号写入同样写穿；出口工程读到编辑后的拍号
#[test]
fn time_signature_edit_reaches_export_project() {
    let mut data = data_with_document();
    data.set_time_signatures(vec![(0, 3, 4)]);

    let doc = data.document.as_ref().expect("文档应存在");
    assert_eq!(doc.time_signatures, vec![(0, 3, 4)]);

    let project = LuminoProject::from_midi_document(doc);
    assert_eq!(
        project.time_signatures.first().map(|&(_, n, d)| (n, d)),
        Some((3, 4)),
        "工程拍号必须是编辑后的值: {:?}",
        project.time_signatures
    );
}
