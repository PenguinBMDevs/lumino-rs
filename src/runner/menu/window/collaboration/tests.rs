//! 协作房间工程序列化契约测试
//!
//! REF-DATA-003 / #230：`serialize_room_project` 曾调用
//! `project.apply_tempo_points(tempo)`，把 `EditorData.tempo_points` 覆盖进工程——
//! 这是 tempo 单一权威源整改（审计 §三）漏网的**第 7 处**出口补丁。
//!
//! 整改后的不变量：`EditorData` 的每个 tempo 写入入口都经
//! `sync_tempo_to_document` 写穿，`doc.tempo_changes` 恒为编辑后的值，
//! 所有出口（含协作房间工程）只读 document。本文件把这条钉成回归断言。
//!
//! 落点说明：`serialize_room_project` 是 bin crate 内的私有函数，
//! 根包没有 lib target、`crates/editor/ui` 是另一个 crate，都访问不到它，
//! 因此回归测试只能放在本模块下。

use super::RunnerInner;
use lumino_export::LuminoProject;
use lumino_midi_loader::MidiDocument;
use lumino_project::project::load::load_project_from_bytes;

/// 构造一份已带 tempo 的文档（模拟用户已在速度面板改过 BPM）
fn doc_with_tempo(tempo_changes: &[(u32, f32)]) -> MidiDocument {
    let mut doc = MidiDocument::empty_with_tracks(2, 480);
    doc.tempo_changes = tempo_changes.to_vec();
    doc
}

/// 房间工程必须直接携带 document 里编辑后的 tempo（无补丁也不能丢）
#[test]
fn room_project_bytes_carry_edited_document_tempo() {
    let doc = doc_with_tempo(&[(0, 150.0), (960, 90.5)]);

    let bytes = RunnerInner::serialize_room_project(&doc).expect("序列化房间工程失败");
    let project = load_project_from_bytes(&bytes).expect("回读房间工程失败");

    assert_eq!(
        project.tempo_changes,
        vec![(0, 150.0), (960, 90.5)],
        "协作工程 tempo 必须是 document 的编辑后值（补丁已删除，行为不得回退）"
    );
}

/// 被删掉的补丁对已同步的 document 是 no-op（写回同值），
/// 因此删除它不会改变协作工程内容，也不会改变 host/joiner 的 hash 口径。
#[test]
fn removed_tempo_patch_only_rewrote_identical_values() {
    let doc = doc_with_tempo(&[(0, 150.0), (960, 90.5)]);
    let mut project = LuminoProject::from_midi_document(&doc);
    let synced = project.tempo_changes.clone();

    // 旧补丁写入的就是 document 里已经同步过的同一批值
    project.apply_tempo_points(
        doc.tempo_changes
            .iter()
            .map(|&(tick, bpm)| (tick as f32, bpm as f64)),
    );

    assert_eq!(
        project.tempo_changes, synced,
        "旧补丁写回值与 from_midi_document 读到的值必须逐项相同"
    );
}
