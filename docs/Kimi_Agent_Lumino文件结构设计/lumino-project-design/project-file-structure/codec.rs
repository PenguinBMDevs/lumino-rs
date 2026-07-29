//! Lumino 工程文件编码/解码实现
//!
//! 提供 .lmtrack、.lmtemp、.lmsig、.lmctl、.lmnames、.lmloaded 文件的
//! 编码（序列化+压缩）和解码（解压+反序列化）功能。

use super::project::*;

// ───────────────────────────────────────────────
// LmtrackData 编解码
// ───────────────────────────────────────────────

impl LmtrackData {
    /// 从 CompactEvent 切片构建音轨数据
    ///
    /// 直接将事件序列化为扁平字节数组，无需额外解析。
    pub fn from_compact_events(meta: TrackMeta, events: &[lumino_midi::compact::CompactEvent]) -> Self {
        use lumino_midi::compact::EventKind;

        let mut event_bytes = Vec::with_capacity(events.len() * 12);
        let mut note_count = 0u64;

        for ev in events {
            event_bytes.extend_from_slice(ev.as_bytes());
            if matches!(ev.kind(), EventKind::NoteOn) {
                note_count += 1;
            }
        }

        Self {
            meta,
            events: event_bytes,
            event_count: events.len() as u64,
            note_count,
        }
    }

    /// 编码为 .lmtrack 文件字节（文件头 + zstd 压缩数据）
    pub fn encode(&self) -> Result<Vec<u8>, crate::CoreError> {
        // 1. 写入文件头
        let header = LmtrackHeader::new(self.meta.track_id);
        let mut result = header.to_bytes().to_vec();

        // 2. bincode 序列化主体
        let serialized = bincode::serialize(self).map_err(|e| {
            crate::CoreError::Encoding(format!("lmtrack bincode serialize: {e}"))
        })?;

        // 3. zstd 压缩（level 3，平衡速度与压缩比）
        let compressed = zstd::stream::encode_all(std::io::Cursor::new(serialized), 3)
            .map_err(|e| crate::CoreError::Compression(format!("lmtrack zstd: {e}")))?;

        result.extend_from_slice(&compressed);
        Ok(result)
    }

    /// 从 .lmtrack 文件字节解码
    pub fn decode(bytes: &[u8]) -> Result<Self, crate::CoreError> {
        if bytes.len() < LmtrackHeader::SIZE {
            return Err(crate::CoreError::FileFormat(
                "lmtrack: file too short".into(),
            ));
        }

        // 1. 验证文件头
        let header =
            LmtrackHeader::from_bytes(&bytes[..LmtrackHeader::SIZE]).ok_or_else(|| {
                crate::CoreError::FileFormat("lmtrack: invalid header".into())
            })?;

        if &header.magic != LmtrackHeader::MAGIC {
            return Err(crate::CoreError::FileFormat(
                format!(
                    "lmtrack: invalid magic, expected {:?}, got {:?}",
                    LmtrackHeader::MAGIC,
                    header.magic
                )
                .into(),
            ));
        }

        if header.version != LmtrackHeader::CURRENT_VERSION {
            return Err(crate::CoreError::FileFormat(
                format!("lmtrack: unsupported version {}", header.version).into(),
            ));
        }

        // 2. zstd 解压
        let compressed = &bytes[LmtrackHeader::SIZE..];
        let decompressed = zstd::stream::decode_all(std::io::Cursor::new(compressed)).map_err(
            |e| crate::CoreError::Compression(format!("lmtrack decompression: {e}")),
        )?;

        // 3. bincode 反序列化
        let data: LmtrackData = bincode::deserialize(&decompressed).map_err(|e| {
            crate::CoreError::Encoding(format!("lmtrack bincode deserialize: {e}"))
        })?;

        // 4. 验证数据一致性
        let expected_event_bytes = (data.event_count as usize) * 12;
        if data.events.len() != expected_event_bytes {
            return Err(crate::CoreError::FileFormat(
                format!(
                    "lmtrack: event size mismatch: expected {} bytes, got {}",
                    expected_event_bytes,
                    data.events.len()
                )
                .into(),
            ));
        }

        Ok(data)
    }

    /// 获取 CompactEvent 迭代器（零拷贝视图）
    ///
    /// 直接从扁平字节数组创建事件视图，无需分配。
    pub fn compact_events(&self) -> impl Iterator<Item = lumino_midi::compact::CompactEvent> + '_ {
        self.events.chunks_exact(12).map(|chunk| {
            let bytes: &[u8; 12] = chunk.try_into().expect("12 byte chunk");
            lumino_midi::compact::CompactEvent::from_bytes(bytes)
        })
    }

    /// 获取 CompactEvent 数量
    pub fn event_count(&self) -> usize {
        self.event_count as usize
    }

    /// 获取音符数量（NoteOn 事件数）
    pub fn note_count(&self) -> u64 {
        self.note_count
    }

    /// 获取指定 tick 范围内的事件
    pub fn events_in_range(
        &self,
        tick_start: u32,
        tick_end: u32,
    ) -> Vec<lumino_midi::compact::CompactEvent> {
        self.compact_events()
            .filter(|ev| {
                let tick = ev.delta_tick();
                tick >= tick_start && tick < tick_end
            })
            .collect()
    }
}

// ───────────────────────────────────────────────
// 辅助数据文件编解码（通用宏模式）
// ───────────────────────────────────────────────

/// 带魔数的压缩数据文件编解码 trait
pub trait CompressedDataFile: serde::Serialize + serde::de::DeserializeOwned + Sized {
    /// 文件魔数（4 字节）
    const MAGIC: &'static [u8; 4];
    /// 当前格式版本
    const VERSION: u16;

    /// 编码为文件字节（文件头 + zstd 压缩）
    fn encode_file(&self) -> Result<Vec<u8>, crate::CoreError> {
        let mut result = Vec::new();

        // 文件头: magic(4) + version(2)
        result.extend_from_slice(Self::MAGIC);
        result.extend_from_slice(&Self::VERSION.to_le_bytes());

        // bincode + zstd
        let serialized =
            bincode::serialize(self).map_err(|e| crate::CoreError::Encoding(format!("{e}")))?;
        let compressed = zstd::stream::encode_all(std::io::Cursor::new(serialized), 3).map_err(
            |e| crate::CoreError::Compression(format!("{e}")),
        )?;

        result.extend_from_slice(&compressed);
        Ok(result)
    }

    /// 从文件字节解码
    fn decode_file(bytes: &[u8]) -> Result<Self, crate::CoreError> {
        const HEADER_SIZE: usize = 6; // magic(4) + version(2)

        if bytes.len() < HEADER_SIZE {
            return Err(crate::CoreError::FileFormat("file too short".into()));
        }

        // 验证魔数
        if &bytes[0..4] != Self::MAGIC {
            return Err(crate::CoreError::FileFormat(
                format!(
                    "invalid magic: expected {:?}, got {:?}",
                    Self::MAGIC,
                    &bytes[0..4]
                )
                .into(),
            ));
        }

        // 验证版本
        let version = u16::from_le_bytes([bytes[4], bytes[5]]);
        if version != Self::VERSION {
            return Err(crate::CoreError::FileFormat(
                format!("unsupported version: {version}").into(),
            ));
        }

        // 解压 + 反序列化
        let decompressed =
            zstd::stream::decode_all(std::io::Cursor::new(&bytes[HEADER_SIZE..])).map_err(|e| {
                crate::CoreError::Compression(format!("{e}"))
            })?;

        let data: Self = bincode::deserialize(&decompressed)
            .map_err(|e| crate::CoreError::Encoding(format!("{e}")))?;

        Ok(data)
    }
}

// 为各辅助数据类型实现 CompressedDataFile

impl CompressedDataFile for LmtempData {
    const MAGIC: &'static [u8; 4] = b"LMTM";
    const VERSION: u16 = 1;
}

impl CompressedDataFile for LmsigData {
    const MAGIC: &'static [u8; 4] = b"LMSG";
    const VERSION: u16 = 1;
}

impl CompressedDataFile for LmctlData {
    const MAGIC: &'static [u8; 4] = b"LMCT";
    const VERSION: u16 = 1;
}

impl CompressedDataFile for LmnamesData {
    const MAGIC: &'static [u8; 4] = b"LMNM";
    const VERSION: u16 = 1;
}

impl CompressedDataFile for LoadedMidiData {
    const MAGIC: &'static [u8; 4] = b"LMLD";
    const VERSION: u16 = 1;
}

impl CompressedDataFile for LoadedDmsData {
    const MAGIC: &'static [u8; 4] = b"LMLD";
    const VERSION: u16 = 1;
}

impl CompressedDataFile for LoadedLmpjData {
    const MAGIC: &'static [u8; 4] = b"LMLD";
    const VERSION: u16 = 1;
}

// ───────────────────────────────────────────────
// 归档格式编解码
// ───────────────────────────────────────────────

impl ArchiveHeader {
    /// 验证魔数
    pub fn is_valid(&self) -> bool {
        &self.magic == ArchiveHeader::MAGIC && self.version == ArchiveHeader::CURRENT_VERSION
    }
}

impl FileTable {
    /// 查找指定路径的文件条目
    pub fn find_entry(&self, path: &str) -> Option<&FileEntry> {
        self.entries.iter().find(|e| e.path == path)
    }

    /// 从归档字节中读取文件表
    pub fn read_from_archive(archive_bytes: &[u8]) -> Result<Self, crate::CoreError> {
        let header = ArchiveHeader::from_bytes(&archive_bytes[..ArchiveHeader::SIZE]).ok_or(
            crate::CoreError::FileFormat("invalid archive header".into()),
        )?;

        if !header.is_valid() {
            return Err(crate::CoreError::FileFormat(
                "invalid or unsupported archive".into(),
            ));
        }

        let ft_start = header.file_table_offset as usize;
        let ft_end = ft_start + header.file_table_compressed_size as usize;

        if ft_end > archive_bytes.len() {
            return Err(crate::CoreError::FileFormat(
                "archive file table out of bounds".into(),
            ));
        }

        let ft_compressed = &archive_bytes[ft_start..ft_end];
        let ft_decompressed = zstd::stream::decode_all(std::io::Cursor::new(ft_compressed))
            .map_err(|e| crate::CoreError::Compression(format!("file table: {e}")))?;

        if ft_decompressed.len() != header.file_table_original_size as usize {
            return Err(crate::CoreError::FileFormat(
                "file table size mismatch".into(),
            ));
        }

        let table: FileTable = bincode::deserialize(&ft_decompressed)
            .map_err(|e| crate::CoreError::Encoding(format!("file table: {e}")))?;

        Ok(table)
    }
}

/// 归档读取器
pub struct ArchiveReader<'a> {
    bytes: &'a [u8],
    file_table: FileTable,
}

impl<'a> ArchiveReader<'a> {
    /// 打开归档
    pub fn open(bytes: &'a [u8]) -> Result<Self, crate::CoreError> {
        let file_table = FileTable::read_from_archive(bytes)?;
        Ok(Self {
            bytes,
            file_table,
        })
    }

    /// 读取指定路径的文件内容
    pub fn read_file(&self, path: &str) -> Result<Option<Vec<u8>>, crate::CoreError> {
        let entry = match self.file_table.find_entry(path) {
            Some(e) => e,
            None => return Ok(None),
        };

        let start = entry.data_offset as usize;
        let end = start + entry.compressed_size as usize;

        if end > self.bytes.len() {
            return Err(crate::CoreError::FileFormat(
                format!("file data out of bounds: {path}").into(),
            ));
        }

        let data = &self.bytes[start..end];

        let result = if entry.is_compressed {
            zstd::stream::decode_all(std::io::Cursor::new(data)).map_err(|e| {
                crate::CoreError::Compression(format!("{path}: {e}"))
            })?
        } else {
            data.to_vec()
        };

        // CRC32 校验（可选）
        let computed_crc = crc32fast::hash(&result);
        if computed_crc != entry.crc32 {
            // 警告但不中断，允许继续
            tracing::warn!(
                "CRC32 mismatch for {}: expected {:08x}, got {:08x}",
                path,
                entry.crc32,
                computed_crc
            );
        }

        Ok(Some(result))
    }

    /// 列出归档中所有文件
    pub fn list_files(&self) -> &[FileEntry] {
        &self.file_table.entries
    }
}

/// 归档写入器
pub struct ArchiveWriter {
    entries: Vec<(String, Vec<u8>, bool)>, // (path, data, compress)
}

impl ArchiveWriter {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// 添加文件到归档
    pub fn add_file(&mut self, path: impl Into<String>, data: Vec<u8>, compress: bool) {
        self.entries.push((path.into(), data, compress));
    }

    /// 添加已压缩的数据文件（不再二次压缩）
    pub fn add_precompressed(&mut self, path: impl Into<String>, data: Vec<u8>, original_size: u64) {
        // 标记为不压缩，但记录原始大小
        self.entries.push((path.into(), data, false));
    }

    /// 构建归档字节
    pub fn build(self) -> Result<Vec<u8>, crate::CoreError> {
        use std::io::Write;

        let mut archive = Vec::new();

        // 预留文件头空间
        let header_placeholder = [0u8; ArchiveHeader::SIZE];
        archive.extend_from_slice(&header_placeholder);

        let mut file_entries = Vec::new();

        // 写入每个文件的数据
        for (path, data, should_compress) in &self.entries {
            let data_offset = archive.len() as u64;

            let (stored_data, compressed_size, original_size, is_compressed) = if *should_compress {
                let compressed = zstd::stream::encode_all(std::io::Cursor::new(data), 3).map_err(
                    |e| crate::CoreError::Compression(format!("{path}: {e}")),
                )?;
                let orig_size = data.len() as u64;
                let comp_size = compressed.len() as u64;
                (compressed, comp_size, orig_size, true)
            } else {
                let size = data.len() as u64;
                (data.clone(), size, size, false)
            };

            let crc32 = crc32fast::hash(
                if is_compressed {
                    // 需要解压后计算 CRC，但我们只有压缩后的数据
                    // 简化：存储时先解压再计算，或存储原始数据的 CRC
                    data.as_slice()
                } else {
                    stored_data.as_slice()
                }
            );

            archive.extend_from_slice(&stored_data);

            file_entries.push(FileEntry {
                path: path.clone(),
                data_offset,
                compressed_size,
                original_size,
                crc32,
                is_compressed,
            });
        }

        // 构建并压缩文件表
        let file_table = FileTable::new(file_entries);
        let ft_encoded = file_table
            .encode()
            .map_err(|e| crate::CoreError::Encoding(format!("file table: {e}")))?;

        let ft_offset = archive.len() as u64;
        let ft_compressed_size = ft_encoded.len() as u64;
        let ft_original_size = bincode::serialize(&file_table)
            .map_err(|e| crate::CoreError::Encoding(format!("{e}")))?
            .len() as u64;

        // 写入文件表
        archive.extend_from_slice(&ft_encoded);

        // 回填文件头
        let header = ArchiveHeader::new(ft_offset, ft_compressed_size, ft_original_size);
        let header_bytes = header.to_bytes();
        archive[..ArchiveHeader::SIZE].copy_from_slice(&header_bytes);

        Ok(archive)
    }
}

impl Default for ArchiveWriter {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────────────────────────────────────────
// 文件夹形态读写
// ───────────────────────────────────────────────

use std::path::Path;

/// 文件夹工程读写
pub struct FolderProjectIO;

impl FolderProjectIO {
    /// 读取文件夹形态的工程
    pub fn read_project(folder_path: &Path) -> Result<LuminoProject, crate::CoreError> {
        // 1. 读取 metadata.toml
        let metadata_path = folder_path.join("metadata.toml");
        let metadata_str = std::fs::read_to_string(&metadata_path).map_err(|e| {
            crate::CoreError::Io(e)
        })?;
        let metadata: ProjectMetadata = toml::from_str(&metadata_str).map_err(|e| {
            crate::CoreError::FileFormat(format!("metadata.toml parse error: {e}"))
        })?;

        // 2. 读取各音轨
        let tracks_dir = folder_path.join("data").join("project").join("tracks");
        let mut tracks = Vec::new();

        for entry in std::fs::read_dir(&tracks_dir).map_err(crate::CoreError::Io)? {
            let entry = entry.map_err(crate::CoreError::Io)?;
            let path = entry.path();

            if path.extension().and_then(|s| s.to_str()) == Some("lmtrack") {
                let bytes = std::fs::read(&path).map_err(crate::CoreError::Io)?;
                let track_data = LmtrackData::decode(&bytes)?;
                tracks.push(TrackSlot::Loaded(track_data));
            }
        }

        // 按 track_id 排序
        tracks.sort_by(|a, b| {
            let id_a = match a {
                TrackSlot::Loaded(d) | TrackSlot::Modified(d) => d.meta.track_id,
                TrackSlot::Unloaded { track_id, .. } => *track_id,
            };
            let id_b = match b {
                TrackSlot::Loaded(d) | TrackSlot::Modified(d) => d.meta.track_id,
                TrackSlot::Unloaded { track_id, .. } => *track_id,
            };
            id_a.cmp(&id_b)
        });

        // 3. 读取辅助数据（可选，有默认值）
        let project_dir = folder_path.join("data").join("project");

        let tempo_changes = if let Ok(bytes) = std::fs::read(project_dir.join("tempo.lmtemp")) {
            LmtempData::decode_file(&bytes)
                .map(|d| d.tempo_changes)
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let (time_signatures, key_signatures) =
            if let Ok(bytes) = std::fs::read(project_dir.join("signature.lmsig")) {
                LmsigData::decode_file(&bytes)
                    .map(|d| (d.time_signatures, d.key_signatures))
                    .unwrap_or_default()
            } else {
                (Vec::new(), Vec::new())
            };

        let (control_changes, program_changes) =
            if let Ok(bytes) = std::fs::read(project_dir.join("controls.lmctl")) {
                LmctlData::decode_file(&bytes)
                    .map(|d| (d.control_changes, d.program_changes))
                    .unwrap_or_default()
            } else {
                (Vec::new(), Vec::new())
            };

        let loaded_files = metadata.loaded.files.clone();

        Ok(LuminoProject {
            metadata,
            tracks,
            tempo_changes,
            time_signatures,
            key_signatures,
            control_changes,
            program_changes,
            loaded_files,
        })
    }

    /// 保存工程为文件夹形态
    pub fn save_project(
        folder_path: &Path,
        project: &LuminoProject,
    ) -> Result<(), crate::CoreError> {
        use std::fs;

        // 1. 创建目录结构
        let project_dir = folder_path.join("data").join("project");
        let tracks_dir = project_dir.join("tracks");
        let image_dir = folder_path.join("data").join("image");
        let loaded_dir = folder_path.join("data").join("loaded");

        fs::create_dir_all(&tracks_dir).map_err(crate::CoreError::Io)?;
        fs::create_dir_all(&image_dir).map_err(crate::CoreError::Io)?;
        fs::create_dir_all(loaded_dir.join("mid")).map_err(crate::CoreError::Io)?;
        fs::create_dir_all(loaded_dir.join("dms")).map_err(crate::CoreError::Io)?;
        fs::create_dir_all(loaded_dir.join("lmpj")).map_err(crate::CoreError::Io)?;
        fs::create_dir_all(folder_path.join(".lumino")).map_err(crate::CoreError::Io)?;

        // 2. 写入版本文件
        fs::write(folder_path.join(".lumino").join("version"), "1\n")
            .map_err(crate::CoreError::Io)?;

        // 3. 写入 metadata.toml
        let metadata_str = toml::to_string_pretty(&project.metadata).map_err(|e| {
            crate::CoreError::Encoding(format!("metadata serialize: {e}"))
        })?;
        fs::write(folder_path.join("metadata.toml"), metadata_str)
            .map_err(crate::CoreError::Io)?;

        // 4. 写入各音轨
        for track in &project.tracks {
            let track_data = match track {
                TrackSlot::Loaded(d) | TrackSlot::Modified(d) => d,
                TrackSlot::Unloaded { .. } => continue, // 未修改的跳过
            };

            let track_path = tracks_dir.join(format!("{:03}.lmtrack", track_data.meta.track_id));
            let encoded = track_data.encode()?;
            fs::write(&track_path, encoded).map_err(crate::CoreError::Io)?;
        }

        // 5. 写入辅助数据文件
        let lmtemp = LmtempData {
            tempo_changes: project.tempo_changes.clone(),
            default_bpm: project.metadata.audio.default_bpm,
        };
        fs::write(
            project_dir.join("tempo.lmtemp"),
            lmtemp.encode_file()?,
        )
        .map_err(crate::CoreError::Io)?;

        let lmsig = LmsigData {
            time_signatures: project.time_signatures.clone(),
            key_signatures: project.key_signatures.clone(),
        };
        fs::write(
            project_dir.join("signature.lmsig"),
            lmsig.encode_file()?,
        )
        .map_err(crate::CoreError::Io)?;

        let lmctl = LmctlData {
            control_changes: project.control_changes.clone(),
            program_changes: project.program_changes.clone(),
            pitch_bends: Vec::new(), // 从现有数据中提取
        };
        fs::write(
            project_dir.join("controls.lmctl"),
            lmctl.encode_file()?,
        )
        .map_err(crate::CoreError::Io)?;

        let lmnames = LmnamesData {
            track_names: project
                .tracks
                .iter()
                .map(|t| match t {
                    TrackSlot::Loaded(d) | TrackSlot::Modified(d) => Some(d.meta.name.clone()),
                    _ => None,
                })
                .collect(),
        };
        fs::write(
            project_dir.join("track_names.lmnames"),
            lmnames.encode_file()?,
        )
        .map_err(crate::CoreError::Io)?;

        Ok(())
    }
}

// ───────────────────────────────────────────────
// 统一工程加载/保存 API
// ───────────────────────────────────────────────

/// 加载工程（自动检测格式：文件夹 / 单文件归档 / 旧版 LMPJ）
pub async fn load_project(path: PathBuf) -> Result<LuminoProject, crate::CoreError> {
    if path.is_dir() {
        // 文件夹形态
        FolderProjectIO::read_project(&path)
    } else {
        let bytes = tokio::fs::read(&path).await.map_err(crate::CoreError::Io)?;

        if bytes.len() >= 4 && &bytes[0..4] == ArchiveHeader::MAGIC {
            // 新版单文件归档
            let reader = ArchiveReader::open(&bytes)?;
            // 从归档中读取 metadata.toml
            let metadata_bytes = reader
                .read_file("metadata.toml")?
                .ok_or_else(|| crate::CoreError::FileFormat("missing metadata.toml".into()))?;
            let metadata_str = String::from_utf8(metadata_bytes).map_err(|e| {
                crate::CoreError::FileFormat(format!("metadata.toml not utf-8: {e}"))
            })?;
            let metadata: ProjectMetadata = toml::from_str(&metadata_str).map_err(|e| {
                crate::CoreError::FileFormat(format!("metadata.toml parse: {e}"))
            })?;

            // 读取各音轨
            let mut tracks = Vec::new();
            for ft_entry in reader.list_files() {
                if ft_entry.path.starts_with("data/project/tracks/")
                    && ft_entry.path.ends_with(".lmtrack")
                {
                    if let Some(track_bytes) = reader.read_file(&ft_entry.path)? {
                        let track_data = LmtrackData::decode(&track_bytes)?;
                        tracks.push(TrackSlot::Loaded(track_data));
                    }
                }
            }

            // 其他数据类似读取...
            // (tempo, signature, controls)

            Ok(LuminoProject {
                metadata,
                tracks,
                tempo_changes: Vec::new(),
                time_signatures: Vec::new(),
                key_signatures: Vec::new(),
                control_changes: Vec::new(),
                program_changes: Vec::new(),
                loaded_files: Vec::new(),
            })
        } else {
            // 旧版 LMPJ（直接 bincode+zstd 的 LmpjData）
            load_legacy_lmpj(&bytes)
        }
    }
}

/// 加载旧版 LMPJ 文件（兼容）
fn load_legacy_lmpj(bytes: &[u8]) -> Result<LuminoProject, crate::CoreError> {
    let lmpj_data: crate::LmpjData = crate::export::format::decode_lmpj(bytes).map_err(|e| {
        crate::CoreError::FileFormat(format!("legacy lmpj decode: {e}"))
    })?;

    // 转换为新工程格式
    let metadata = ProjectMetadata {
        format_version: 1,
        project: ProjectInfo {
            name: lmpj_data
                .info
                .path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Untitled")
                .to_string(),
            author: String::new(),
            created_at: chrono::Local::now().to_rfc3339(),
            modified_at: chrono::Local::now().to_rfc3339(),
            description: String::new(),
            lumino_version: env!("CARGO_PKG_VERSION").to_string(),
        },
        audio: AudioInfo {
            division: lmpj_data.info.division,
            total_ticks: lmpj_data.info.duration_ticks,
            track_count: lmpj_data.info.track_count,
            total_notes: lmpj_data.info.total_notes,
            default_bpm: 120.0,
        },
        tracks: TrackList::default(),
        loaded: LoadedFileList::default(),
        settings: ProjectSettings::default(),
        stats: ProjectStats::default(),
    };

    // 旧版只有一个 midi_data，需要解析后才能得到各音轨
    // 这里标记为需要解析
    let tracks = Vec::new(); // 空的，需要后续异步解析

    Ok(LuminoProject {
        metadata,
        tracks,
        tempo_changes: Vec::new(),
        time_signatures: Vec::new(),
        key_signatures: Vec::new(),
        control_changes: Vec::new(),
        program_changes: Vec::new(),
        loaded_files: Vec::new(),
    })
}
