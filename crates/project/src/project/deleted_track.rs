//! 已删除音轨缓存（.lmdeltrack）格式定义与读写
//!
//! 当用户删除音轨时，音轨数据被缓存到硬盘以便恢复。
//! 缓存文件采用 lumino 工程文件通用压缩标准（zstd level 3），
//! 内部包含两个虚拟文件：
//! - `trackdata.toml`：TOML 格式的音轨元数据（人类可读）
//! - `track.lmdeltdata`：bincode 格式的音符数据
//!
//! 文件结构（压缩前）：
//! ```text
//! [魔数 b"LMDT" 4B][version u16 2B]
//! [toml_len u32 4B][toml 内容 UTF-8]
//! [data_len u32 4B][data 内容 bincode]
//! ```
//! 整个文件用 zstd level 3 压缩（与 .lmtrack 一致）。

#[cfg(test)]
mod tests;

use std::path::{Path, PathBuf};

use lumino_core::error::{CoreError, Result};

/// 已删除音轨缓存魔数：`LMDT`（Lumino Deleted Track）
const MAGIC: &[u8; 4] = b"LMDT";

/// 已删除音轨缓存格式版本
const VERSION: u16 = 1;

/// zstd 压缩级别（与 .lmtrack 一致，level 3 速度优先）
const ZSTD_LEVEL: i32 = 3;

/// 已删除音轨元数据（对应 `trackdata.toml`）
///
/// 记录音轨在删除时的关键信息，用于恢复时重建音轨。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeletedTrackMetadata {
    /// 音轨编号（删除时的原始 ID，恢复时优先使用）
    pub track_id: u16,
    /// 音轨名称（用于文件命名与显示）
    pub track_name: String,
    /// MIDI 端口号（0-25 映射到 A-Z）
    pub port: u8,
    /// MIDI 通道号（0-15）
    pub channel: u8,
    /// 音符总数（用于显示）
    pub note_count: u64,
    /// 删除时间（ISO 8601 格式字符串）
    pub deleted_at: String,
    /// 在 sidebar.tracks 中的原始位置索引（恢复时优先放回此位置）
    pub original_index: usize,
    /// 是否为鼓音轨
    pub is_drum: bool,
    /// 此音轨最后一个事件的 tick（用于恢复时设置 max_tick）
    pub max_tick: u32,
}

/// 已删除音轨数据（对应 `track.lmdeltdata`）
///
/// 仅存储 NoteOn 事件足够恢复音轨——end_tick 由 gate 推算。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeletedTrackData {
    /// NoteOn 事件列表（已按 start_tick 排序）
    pub notes: Vec<DeletedNote>,
}

/// 已删除音符（NoteOn 事件，足够恢复音轨）
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct DeletedNote {
    /// 起始 tick
    pub start_tick: u32,
    /// 结束 tick（包含，便于恢复时直接使用）
    pub end_tick: u32,
    /// 音高（0-127）
    pub key: u8,
    /// 力度（0-127）
    pub velocity: u8,
    /// 通道号（0-15）
    pub channel: u8,
    /// 端口号（0-15）
    pub port: u8,
}

/// 已删除音轨缓存条目（列表展示用）
#[derive(Debug, Clone)]
pub struct DeletedTrackEntry {
    /// 缓存文件路径
    pub path: PathBuf,
    /// 缓存文件名（不含路径，含扩展名）
    pub filename: String,
    /// 元数据
    pub metadata: DeletedTrackMetadata,
}

/// 编码为 .lmdeltrack 字节（zstd 压缩）
///
/// 内部结构：魔数 + version + toml_len + toml + data_len + data，
/// 整体用 zstd level 3 压缩。
fn encode(meta: &DeletedTrackMetadata, data: &DeletedTrackData) -> Result<Vec<u8>> {
    let mut raw = Vec::new();
    raw.extend_from_slice(MAGIC);
    raw.extend_from_slice(&VERSION.to_le_bytes());

    // trackdata.toml 部分
    let toml_str = toml::to_string_pretty(meta).map_err(CoreError::from)?;
    let toml_bytes = toml_str.as_bytes();
    raw.extend_from_slice(&(toml_bytes.len() as u32).to_le_bytes());
    raw.extend_from_slice(toml_bytes);

    // track.lmdeltdata 部分（bincode）
    let data_bytes = bincode::serialize(data).map_err(CoreError::from)?;
    raw.extend_from_slice(&(data_bytes.len() as u32).to_le_bytes());
    raw.extend_from_slice(&data_bytes);

    // 整体 zstd 压缩
    let compressed = zstd::stream::encode_all(std::io::Cursor::new(raw), ZSTD_LEVEL)
        .map_err(|e| CoreError::Compression(format!("deleted_track zstd: {e}")))?;
    Ok(compressed)
}

/// 从 .lmdeltrack 字节解码（zstd 解压）
fn decode(bytes: &[u8]) -> Result<(DeletedTrackMetadata, DeletedTrackData)> {
    let decompressed = zstd::stream::decode_all(std::io::Cursor::new(bytes))
        .map_err(|e| CoreError::Compression(format!("deleted_track decompression: {e}")))?;

    if decompressed.len() < 10 {
        return Err(CoreError::FileFormat(
            "deleted_track: too short for header".into(),
        ));
    }
    if &decompressed[0..4] != MAGIC {
        return Err(CoreError::FileFormat("deleted_track: invalid magic".into()));
    }
    let version = u16::from_le_bytes([decompressed[4], decompressed[5]]);
    if version != VERSION {
        return Err(CoreError::FileFormat(format!(
            "deleted_track: unsupported version {version}"
        )));
    }

    let mut cursor = 6usize;
    // 读取 toml 部分
    if cursor + 4 > decompressed.len() {
        return Err(CoreError::FileFormat(
            "deleted_track: truncated toml length".into(),
        ));
    }
    let toml_len = read_u32_le(&decompressed[cursor..cursor + 4])? as usize;
    cursor += 4;
    if cursor + toml_len > decompressed.len() {
        return Err(CoreError::FileFormat(
            "deleted_track: truncated toml content".into(),
        ));
    }
    let toml_str = std::str::from_utf8(&decompressed[cursor..cursor + toml_len])
        .map_err(|e| CoreError::Serialization(format!("deleted_track toml utf8: {e}")))?;
    let meta: DeletedTrackMetadata = toml::from_str(toml_str).map_err(CoreError::from)?;
    cursor += toml_len;

    // 读取 data 部分
    if cursor + 4 > decompressed.len() {
        return Err(CoreError::FileFormat(
            "deleted_track: truncated data length".into(),
        ));
    }
    let data_len = read_u32_le(&decompressed[cursor..cursor + 4])? as usize;
    cursor += 4;
    if cursor + data_len > decompressed.len() {
        return Err(CoreError::FileFormat(
            "deleted_track: truncated data content".into(),
        ));
    }
    let data: DeletedTrackData =
        bincode::deserialize(&decompressed[cursor..cursor + data_len]).map_err(CoreError::from)?;

    Ok((meta, data))
}

/// 从 4 字节小端序切片读取 u32（切片长度必须为 4）
fn read_u32_le(bytes: &[u8]) -> Result<u32> {
    let arr: [u8; 4] = bytes
        .try_into()
        .map_err(|_| CoreError::FileFormat("deleted_track: u32 slice length mismatch".into()))?;
    Ok(u32::from_le_bytes(arr))
}

/// 生成缓存文件名（不含路径，含扩展名）
///
/// 规则：优先使用音轨名称；名称为空或非法时回退到 `track_{id}`。
/// 文件名中的非法字符（路径分隔符等）替换为 `_`。
fn build_filename(meta: &DeletedTrackMetadata) -> String {
    let base = if meta.track_name.trim().is_empty() {
        format!("track_{}", meta.track_id)
    } else {
        meta.track_name.clone()
    };
    // 替换文件名非法字符
    let sanitized: String = base
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();
    format!("{sanitized}.lmdeltrack")
}

/// 在缓存目录中寻找不冲突的文件路径（重名时追加 _2、_3...）
fn resolve_unique_path(cache_dir: &Path, filename: &str) -> PathBuf {
    let direct = cache_dir.join(filename);
    if !direct.exists() {
        return direct;
    }
    let stem = filename.strip_suffix(".lmdeltrack").unwrap_or(filename);
    for i in 2..u32::MAX {
        let candidate = cache_dir.join(format!("{stem}_{i}.lmdeltrack"));
        if !candidate.exists() {
            return candidate;
        }
    }
    // 理论不可达，回退
    direct
}

/// 保存已删除音轨到缓存目录
///
/// 在 `cache_dir` 下创建 `.lmdeltrack` 文件。若重名则追加 `_2`、`_3`...
/// 返回最终写入的文件路径。
pub fn save_deleted_track(
    cache_dir: &Path,
    meta: &DeletedTrackMetadata,
    data: &DeletedTrackData,
) -> Result<PathBuf> {
    std::fs::create_dir_all(cache_dir)?;
    let filename = build_filename(meta);
    let path = resolve_unique_path(cache_dir, &filename);
    let bytes = encode(meta, data)?;
    std::fs::write(&path, bytes)?;
    tracing::info!(
        "已删除音轨缓存已写入: {} ({} 音符)",
        path.display(),
        meta.note_count
    );
    Ok(path)
}

/// 从缓存文件加载已删除音轨
pub fn load_deleted_track(path: &Path) -> Result<(DeletedTrackMetadata, DeletedTrackData)> {
    let bytes = std::fs::read(path)?;
    decode(&bytes)
}

/// 列出缓存目录中所有 .lmdeltrack 文件
///
/// 返回按删除时间倒序排列的条目列表（最新的在前）。
pub fn list_deleted_tracks(cache_dir: &Path) -> Result<Vec<DeletedTrackEntry>> {
    if !cache_dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(cache_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("lmdeltrack") {
            continue;
        }
        let filename = match path.file_name().and_then(|n| n.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("读取已删除音轨缓存失败 {}: {}", path.display(), e);
                continue;
            }
        };
        match decode(&bytes) {
            Ok((meta, _)) => entries.push(DeletedTrackEntry {
                path,
                filename,
                metadata: meta,
            }),
            Err(e) => {
                tracing::warn!("解析已删除音轨缓存失败 {}: {}", path.display(), e);
            }
        }
    }
    // 按删除时间倒序（最新的在前）
    entries.sort_by(|a, b| b.metadata.deleted_at.cmp(&a.metadata.deleted_at));
    Ok(entries)
}

/// 永久删除缓存文件（销毁 .lmdeltrack）
pub fn delete_permanently(path: &Path) -> Result<()> {
    if path.exists() {
        std::fs::remove_file(path)?;
        tracing::info!("已永久销毁已删除音轨缓存: {}", path.display());
    }
    Ok(())
}
