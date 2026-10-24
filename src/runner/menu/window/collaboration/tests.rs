//! 协作房间工程序列化契约测试
//!
//! REF-DATA-003 / #230：房间工程序列化曾调用 `project.apply_tempo_points(tempo)`，
//! 把 `EditorData.tempo_points` 覆盖进工程——这是 tempo 单一权威源整改（审计 §三）
//! 漏网的**第 7 处**出口补丁。整改后的不变量：`EditorData` 的每个 tempo 写入入口
//! 都经 `sync_tempo_to_document` 写穿，`doc.tempo_changes` 恒为编辑后的值，
//! 所有出口（含协作房间工程）只读 document。
//!
//! 同卡修掉第二个缺陷：房间工程曾把**序列化时刻**写进归档头与 metadata，
//! host / joiner 两进程字节必然不同，`local == project_hash` 恒假——
//! "本地工程已是最新"分支成了死代码。现在房间工程固定墙钟字段后编码，
//! 上传字节与被 hash 的字节是同一份。
//!
//! 落点说明：`room_project` 是 bin crate 内的私有模块，
//! 根包没有 lib target、`crates/editor/ui` 是另一个 crate，都访问不到它，
//! 因此回归测试只能放在本模块下。

use super::room_project::{self, CREATED_AT_SECS, METADATA_TIMESTAMP, hash_bytes};
use lumino_export::LuminoProject;
use lumino_midi_loader::MidiDocument;
use lumino_project::project::load::load_project_from_bytes;
use lumino_project::{ArchiveHeader, ProjectMetadata, read_file_from_archive};

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

    let bytes = room_project::serialize(&doc).expect("序列化房间工程失败");
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

/// 房间工程不得写入任何墙钟时间：归档头 `created_at` 与 metadata 时间戳必须固定
///
/// 这是「host 与 joiner 算出的 hash 恒失配」的根因——旧实现两者都取序列化时刻。
#[test]
fn room_project_has_no_wall_clock_fields() {
    let doc = doc_with_tempo(&[(0, 120.0)]);
    let bytes = room_project::serialize(&doc).expect("序列化房间工程失败");

    let header = ArchiveHeader::from_bytes(&bytes).expect("解析归档头失败");
    assert_eq!(
        header.created_at, CREATED_AT_SECS,
        "归档头不得写入墙钟时间，否则跨进程字节必然不同"
    );

    let meta_bytes = read_file_from_archive(&bytes, "metadata.toml")
        .expect("读取 metadata 失败")
        .expect("房间工程必须含 metadata.toml");
    let meta_str = std::str::from_utf8(&meta_bytes).expect("metadata.toml 应为 UTF-8");
    let meta = ProjectMetadata::from_toml_str(meta_str).expect("解析 metadata 失败");
    assert_eq!(
        meta.project.created_at, METADATA_TIMESTAMP,
        "metadata.created_at 必须固定，否则两个进程写出的字节不同"
    );
    assert_eq!(
        meta.project.modified_at, METADATA_TIMESTAMP,
        "metadata.modified_at 必须固定"
    );
}

/// 同一份 document 两次序列化必须逐字节一致（跨进程可复现）
#[test]
fn room_project_bytes_are_byte_stable() {
    let doc = doc_with_tempo(&[(0, 150.0), (960, 90.5)]);

    let first = room_project::serialize(&doc).expect("序列化房间工程失败");
    let second = room_project::serialize(&doc).expect("重复序列化失败");

    assert_eq!(
        first, second,
        "房间工程字节必须与墙钟无关（否则 host 上传的 hash 与 joiner 本地重算的永远不等）"
    );
}

/// 内容一致 ⇒ hash 一致（跳过重复下载）；内容不同 ⇒ hash 不同（不漏下载）
#[test]
fn room_project_hash_tracks_content() {
    // host / joiner 各自从同一份工程装载出 document（各自新建工程、各自编码）
    let host_doc = doc_with_tempo(&[(0, 150.0), (960, 90.5)]);
    let joiner_doc = doc_with_tempo(&[(0, 150.0), (960, 90.5)]);
    let host_hash =
        hash_bytes(&room_project::serialize(&host_doc).expect("host 序列化房间工程失败"));
    let joiner_hash =
        hash_bytes(&room_project::serialize(&joiner_doc).expect("joiner 序列化房间工程失败"));

    assert_eq!(
        host_hash, joiner_hash,
        "同内容必须同 hash——否则永远进不了「本地工程已是最新」分支"
    );
    assert_eq!(host_hash.len(), 16, "hash 为 16 位十六进制");

    let edited_doc = doc_with_tempo(&[(0, 151.0), (960, 90.5)]);
    let edited_hash =
        hash_bytes(&room_project::serialize(&edited_doc).expect("序列化房间工程失败"));
    assert_ne!(
        host_hash, edited_hash,
        "内容不同必须 hash 不同——否则会误判「已是最新」而漏下载"
    );
}

/// hash 算法固定为 FNV-1a 64（不随 Rust 版本变化），锁定公开测试向量
#[test]
fn hash_bytes_is_stable_fnv1a64() {
    assert_eq!(hash_bytes(b"hello"), "a430d84680aabd0b");
    assert_eq!(
        hash_bytes(b""),
        "cbf29ce484222325",
        "空串即 FNV offset basis"
    );
}
