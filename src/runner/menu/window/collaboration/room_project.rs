//! 协作房间工程的**内容指纹**序列化
//!
//! 房间工程有两处用途，都要求「同内容 ⇒ 同字节」：
//!
//! 1. host 上传（服务器存下 hash）；
//! 2. joiner 本地重算 hash 与房间 hash 比对，命中则跳过下载（"本地工程已是最新"）。
//!
//! 归档格式里唯一的非确定性来源是**时间戳**：归档头 `created_at`
//! （`archive::build_archive` 取 `SystemTime::now()`）与 `metadata.toml` 的
//! `created_at` / `modified_at`（`LuminoProject::new` → `chrono::Local::now()`）。
//! host 与 joiner 是两个进程、两次序列化，时间戳必然不同，直接对归档字节取 hash
//! 会**恒失配**——joiner 每次都重复下载，"已是最新"分支成为死代码。
//!
//! 本模块把这三处时间统一固定后再编码，因此**上传字节与被 hash 的字节是同一份**，
//! 不存在"另一套口径"。[`serialize`] 产出的字节可直接用于上传与比对。

use lumino_midi_loader::MidiDocument;
use lumino_project::project::save::encode_archive_with_time;

/// 协作房间工程的**内容指纹基准时间**（归档头 `created_at`）
///
/// 固定为 0：归档头时间是写-only 字段（无读取方），固定它不影响任何行为。
pub(super) const CREATED_AT_SECS: u64 = 0;

/// 与 [`CREATED_AT_SECS`] 配套的 metadata 时间戳（RFC3339 纪元零点）
///
/// `metadata.toml` 的 `created_at` / `modified_at` 同样是写-only 字段
/// （App 内无处读取），固定它们不影响任何展示；接收端打开工程后若再次保存，
/// 时间戳会由保存链路（`from_midi_document` → `LuminoProject::new`）重新生成。
pub(super) const METADATA_TIMESTAMP: &str = "1970-01-01T00:00:00+00:00";

/// 计算字节的 hex 内容指纹（用于工程一致性比对）
///
/// 用 FNV-1a 64 位而不是 `std::collections::hash_map::DefaultHasher`：
/// 后者明确不承诺跨 Rust 版本稳定，一旦算法变化会让"跨客户端比对"静默失效
/// （表现为每次都重复下载）。FNV-1a 的算法与常量永久固定。
/// 本函数只做内容比对，不用于安全场景。
pub(super) fn hash_bytes(bytes: &[u8]) -> String {
    const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = FNV_OFFSET_BASIS;
    for &byte in bytes {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("{hash:016x}")
}

/// 把工程序列化为房间工程字节（纯数据操作，可在后台线程执行，不触碰 UI）
///
/// 入参是已克隆出来的 `MidiDocument`：2026-10-20 单一权威源整改（审计 §三）后
/// `EditorData` 的每个 tempo 写入入口都写穿到 `doc.tempo_changes`，
/// 出口只读 document 即可，**不再单独传 tempo 补丁**。
///
/// 全程内存编码（不走临时文件），且**逐字节确定**：内容相同的工程在任何进程、
/// 任何时刻都产出相同字节。返回 None 表示序列化失败。
pub(super) fn serialize(doc: &MidiDocument) -> Option<Vec<u8>> {
    let project = canonical_project(doc);
    match encode_archive_with_time(&project, CREATED_AT_SECS) {
        Ok(bytes) => Some(bytes),
        Err(e) => {
            tracing::warn!("协作: 序列化房间工程为归档失败，无法上传: {e}");
            None
        }
    }
}

/// 构造用于**内容指纹**的房间工程：抹掉全部墙钟字段
///
/// `from_midi_document` 每次都新建工程，`metadata.project.created_at` /
/// `modified_at` 取当时本地时间（RFC3339，带亚秒）——这是归档字节里除文件头
/// `created_at` 之外仅有的非确定性来源，必须一并归一化。
fn canonical_project(doc: &MidiDocument) -> lumino_export::LuminoProject {
    let mut project = lumino_export::LuminoProject::from_midi_document(doc);
    project.metadata.project.created_at = METADATA_TIMESTAMP.to_string();
    project.metadata.project.modified_at = METADATA_TIMESTAMP.to_string();
    project
}
