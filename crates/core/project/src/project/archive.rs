//! LMPJ 归档文件格式定义与读写
//!
//! 单文件形态是文件夹形态的打包集合体，使用自定义轻量级归档格式。

use crc32fast::Hasher;

use lumino_core::error::{CoreError, Result};
use lumino_core::{tr, trf};

/// 从指定偏移读取固定长度的小端字节数组
fn read_le_bytes<const N: usize>(bytes: &[u8], offset: usize) -> Result<[u8; N]> {
    bytes
        .get(offset..offset + N)
        .ok_or_else(|| CoreError::FileFormat(format!("read at {offset}: out of bounds")))?
        .try_into()
        .map_err(|_| CoreError::FileFormat(format!("read at {offset}: expected {N} bytes")))
}

/// 从指定偏移读取 u16（小端）
fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16> {
    read_le_bytes::<2>(bytes, offset).map(u16::from_le_bytes)
}

/// 从指定偏移读取 u32（小端）
fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32> {
    read_le_bytes::<4>(bytes, offset).map(u32::from_le_bytes)
}

/// 从指定偏移读取 u64（小端）
fn read_u64_le(bytes: &[u8], offset: usize) -> Result<u64> {
    read_le_bytes::<8>(bytes, offset).map(u64::from_le_bytes)
}

/// 归档格式版本（当前仅支持 v1）。
pub const ARCHIVE_VERSION: u16 = 1;

/// 支持的压缩标志：0x01 = zstd。
pub const ARCHIVE_COMPRESSION_ZSTD: u8 = 0x01;

/// 单个部件解压后的上限（2 GiB）：防御损坏文件头 / zstd 炸弹导致的无限分配。
const MAX_DECOMPRESSED_PART_BYTES: u64 = 1 << 31;

/// 按 `offset/len` 取切片：全部 checked 运算 + 上界校验，损坏文件返回
/// `FileFormat` 错误而不是 panic（DEBT-01 #118）。
fn slice_at<'a>(bytes: &'a [u8], offset: u64, len: u64, what: &str) -> Result<&'a [u8]> {
    let start = usize::try_from(offset).map_err(|_| {
        CoreError::FileFormat(trf!(
            "{what}: 偏移 {offset} 超出地址空间",
            what = what,
            offset = offset
        ))
    })?;
    let len = usize::try_from(len).map_err(|_| {
        CoreError::FileFormat(trf!(
            "{what}: 长度 {len} 超出地址空间",
            what = what,
            len = len
        ))
    })?;
    let end = start.checked_add(len).ok_or_else(|| {
        CoreError::FileFormat(trf!(
            "{what}: 偏移 {offset}+{len} 溢出",
            what = what,
            offset = offset,
            len = len
        ))
    })?;
    bytes.get(start..end).ok_or_else(|| {
        CoreError::FileFormat(trf!(
            "{what}: 数据区越界（{start}..{end}，文件共 {total} 字节），文件可能已损坏",
            what = what,
            start = start,
            end = end,
            total = bytes.len()
        ))
    })
}

/// zstd 解压 + 结果校验：带上限、可选原始大小比对。
fn decode_zstd_checked(data: &[u8], what: &str, expected_original_size: u64) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut decoder = zstd::stream::Decoder::new(std::io::Cursor::new(data))
        .map_err(|e| CoreError::Compression(trf!("{what} 解压失败: {e}", what = what, e = e)))?;
    let mut out = Vec::new();
    (&mut decoder)
        .take(MAX_DECOMPRESSED_PART_BYTES + 1)
        .read_to_end(&mut out)
        .map_err(|e| CoreError::Compression(trf!("{what} 解压失败: {e}", what = what, e = e)))?;
    if out.len() as u64 > MAX_DECOMPRESSED_PART_BYTES {
        return Err(CoreError::FileFormat(trf!(
            "{what} 解压后超过上限（{MAX_DECOMPRESSED_PART_BYTES} 字节），文件可能已损坏",
            what = what,
            MAX_DECOMPRESSED_PART_BYTES = MAX_DECOMPRESSED_PART_BYTES
        )));
    }
    if expected_original_size != 0 && out.len() as u64 != expected_original_size {
        return Err(CoreError::FileFormat(trf!(
            "{what} 大小与文件头不一致（期望 {expected_original_size}，实际 {actual}），文件可能已损坏",
            what = what,
            expected_original_size = expected_original_size,
            actual = out.len()
        )));
    }
    Ok(out)
}

/// LMPJ 归档文件头
#[derive(Debug, Clone, Copy)]
pub struct ArchiveHeader {
    /// b"LMPJ"
    pub magic: [u8; 4],
    /// 格式版本
    pub version: u16,
    /// 压缩标志: 0x01 = zstd
    pub compression_flags: u8,
    /// 文件表偏移
    pub file_table_offset: u64,
    /// 文件表压缩后大小
    pub file_table_compressed_size: u64,
    /// 文件表原始大小
    pub file_table_original_size: u64,
    /// 创建时间戳 (unix_secs)
    pub created_at: u64,
    /// 保留字段
    pub _reserved: [u8; 16],
}

impl ArchiveHeader {
    /// 文件头大小: 4 + 2 + 1 + 8 + 8 + 8 + 8 + 16 = 55 bytes
    pub const SIZE: usize = 55;

    /// 编码为字节数组
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..4].copy_from_slice(&self.magic);
        buf[4..6].copy_from_slice(&self.version.to_le_bytes());
        buf[6] = self.compression_flags;
        buf[7..15].copy_from_slice(&self.file_table_offset.to_le_bytes());
        buf[15..23].copy_from_slice(&self.file_table_compressed_size.to_le_bytes());
        buf[23..31].copy_from_slice(&self.file_table_original_size.to_le_bytes());
        buf[31..39].copy_from_slice(&self.created_at.to_le_bytes());
        buf[39..55].copy_from_slice(&self._reserved);
        buf
    }

    /// 从字节数组解码
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < Self::SIZE {
            return Err(CoreError::FileFormat("archive header: too short".into()));
        }
        let mut magic = [0u8; 4];
        magic.copy_from_slice(&bytes[0..4]);
        let version = read_u16_le(bytes, 4)?;
        let compression_flags = bytes[6];
        let file_table_offset = read_u64_le(bytes, 7)?;
        let file_table_compressed_size = read_u64_le(bytes, 15)?;
        let file_table_original_size = read_u64_le(bytes, 23)?;
        let created_at = read_u64_le(bytes, 31)?;
        let mut _reserved = [0u8; 16];
        _reserved.copy_from_slice(&bytes[39..55]);
        Ok(Self {
            magic,
            version,
            compression_flags,
            file_table_offset,
            file_table_compressed_size,
            file_table_original_size,
            created_at,
            _reserved,
        })
    }
}

/// 归档文件表条目
#[derive(Debug, Clone)]
pub struct FileEntry {
    /// 文件路径（相对路径，UTF-8）
    pub path: String,
    /// 数据在归档中的偏移
    pub data_offset: u64,
    /// 压缩后大小
    pub compressed_size: u64,
    /// 原始大小
    pub original_size: u64,
    /// CRC32 校验值
    pub crc32: u32,
    /// 是否压缩
    pub is_compressed: bool,
}

impl FileEntry {
    /// 编码为字节（用于文件表序列化）
    pub fn encode(&self) -> Vec<u8> {
        let path_bytes = self.path.as_bytes();
        let mut result = Vec::with_capacity(2 + path_bytes.len() + 8 + 8 + 8 + 4 + 1);

        let path_len = path_bytes.len() as u16;
        result.extend_from_slice(&path_len.to_le_bytes());
        result.extend_from_slice(path_bytes);
        result.extend_from_slice(&self.data_offset.to_le_bytes());
        result.extend_from_slice(&self.compressed_size.to_le_bytes());
        result.extend_from_slice(&self.original_size.to_le_bytes());
        result.extend_from_slice(&self.crc32.to_le_bytes());
        result.push(if self.is_compressed { 1 } else { 0 });

        result
    }

    /// 从字节解码
    pub fn decode(bytes: &[u8]) -> Result<(Self, usize)> {
        if bytes.len() < 2 {
            return Err(CoreError::FileFormat("file entry: too short".into()));
        }
        let path_len = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
        let mut pos: usize = 2;

        // 固定字段 = data_offset(8) + compressed_size(8) + original_size(8) + crc32(4) + flag(1)
        const FIXED_FIELDS: usize = 8 + 8 + 8 + 4 + 1;
        let needed = pos
            .checked_add(path_len)
            .and_then(|p| p.checked_add(FIXED_FIELDS))
            .ok_or_else(|| CoreError::FileFormat("file entry: length overflow".into()))?;
        if bytes.len() < needed {
            return Err(CoreError::FileFormat(trf!(
                "file entry: incomplete（需要 {needed} 字节，实际 {actual}）",
                needed = needed,
                actual = bytes.len()
            )));
        }

        let path = String::from_utf8(bytes[pos..pos + path_len].to_vec())
            .map_err(|e| CoreError::FileFormat(format!("file entry path: {e}")))?;
        pos += path_len;

        let data_offset = read_u64_le(bytes, pos)?;
        pos += 8;
        let compressed_size = read_u64_le(bytes, pos)?;
        pos += 8;
        let original_size = read_u64_le(bytes, pos)?;
        pos += 8;
        let crc32 = read_u32_le(bytes, pos)?;
        pos += 4;
        let is_compressed = bytes[pos] != 0;
        pos += 1;

        Ok((
            Self {
                path,
                data_offset,
                compressed_size,
                original_size,
                crc32,
                is_compressed,
            },
            pos,
        ))
    }
}

/// 文件表
#[derive(Debug, Clone)]
pub struct FileTable {
    /// 文件表条目列表
    pub entries: Vec<FileEntry>,
}

impl FileTable {
    /// 编码为字节
    pub fn encode(&self) -> Vec<u8> {
        let mut result = Vec::new();
        let count = self.entries.len() as u32;
        result.extend_from_slice(&count.to_le_bytes());
        for entry in &self.entries {
            result.extend_from_slice(&entry.encode());
        }
        result
    }

    /// 从字节解码
    ///
    /// DEBT-01 #118：不再信任 `count` 做预分配（损坏文件可声明 42 亿条 →
    /// 巨量分配）；按剩余字节逐条解码，`pos` 全部 checked 推进。
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 4 {
            return Err(CoreError::FileFormat("file table: too short".into()));
        }
        let count = read_u32_le(bytes, 0)? as usize;
        let mut entries = Vec::new();
        let mut pos = 4usize;

        for _ in 0..count {
            let rest = bytes.get(pos..).ok_or_else(|| {
                CoreError::FileFormat(trf!("file table: 条目区越界（pos={pos}）", pos = pos))
            })?;
            let (entry, consumed) = FileEntry::decode(rest)?;
            pos = pos
                .checked_add(consumed)
                .ok_or_else(|| CoreError::FileFormat("file table: pos overflow".into()))?;
            entries.push(entry);
        }

        Ok(Self { entries })
    }
}

/// 计算 CRC32
pub fn compute_crc32(data: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(data);
    hasher.finalize()
}

/// 归档读取器：解析一次文件头 + 文件表，随后可反复读取文件。
///
/// DEBT-01 #118：所有偏移/长度经 [`slice_at`] 上界校验；读取时校验 CRC32 与
/// 原始大小；损坏文件返回 `FileFormat` 错误（绝不 panic / 巨量预分配）。
pub struct ArchiveReader<'a> {
    bytes: &'a [u8],
    file_table: FileTable,
}

impl<'a> ArchiveReader<'a> {
    /// 解析归档头与文件表。
    pub fn new(bytes: &'a [u8]) -> Result<Self> {
        let header = ArchiveHeader::from_bytes(bytes)?;
        if &header.magic != b"LMPJ" {
            return Err(CoreError::FileFormat(
                tr!("这不是有效的 Lumino 工程文件（魔数不匹配）").into(),
            ));
        }
        if header.version != ARCHIVE_VERSION {
            return Err(CoreError::FileFormat(trf!(
                "不支持的工程文件版本 v{version}（当前支持 v{ARCHIVE_VERSION}）",
                version = header.version,
                ARCHIVE_VERSION = ARCHIVE_VERSION
            )));
        }
        if header.compression_flags != ARCHIVE_COMPRESSION_ZSTD {
            return Err(CoreError::FileFormat(trf!(
                "不支持的归档压缩标志 0x{flags}（仅支持 zstd）",
                flags = format!("{:02X}", header.compression_flags)
            )));
        }

        let ft_data = slice_at(
            bytes,
            header.file_table_offset,
            header.file_table_compressed_size,
            tr!("文件表"),
        )?;
        let decompressed =
            decode_zstd_checked(ft_data, tr!("文件表"), header.file_table_original_size)?;
        let file_table = FileTable::decode(&decompressed)?;

        Ok(Self { bytes, file_table })
    }

    /// 读取归档内的一个文件（不存在返回 `None`）。
    pub fn read(&self, file_path: &str) -> Result<Option<Vec<u8>>> {
        let Some(entry) = self.file_table.entries.iter().find(|e| e.path == file_path) else {
            return Ok(None);
        };

        let data = slice_at(
            self.bytes,
            entry.data_offset,
            entry.compressed_size,
            &trf!("文件 {file_path}", file_path = file_path),
        )?;

        // CRC 覆盖"存储态"字节（压缩后大小），与写入端 `build_archive` 一致
        let crc = compute_crc32(data);
        if crc != entry.crc32 {
            return Err(CoreError::FileFormat(trf!(
                "文件 {file_path} 校验失败（CRC 不匹配），工程可能已损坏",
                file_path = file_path
            )));
        }

        let output = if entry.is_compressed {
            decode_zstd_checked(
                data,
                &trf!("文件 {file_path}", file_path = file_path),
                entry.original_size,
            )?
        } else {
            if entry.original_size != 0 && entry.original_size as usize != data.len() {
                return Err(CoreError::FileFormat(trf!(
                    "文件 {file_path} 大小与文件头不一致（期望 {expected}，实际 {actual}），工程可能已损坏",
                    file_path = file_path,
                    expected = entry.original_size,
                    actual = data.len()
                )));
            }
            data.to_vec()
        };
        Ok(Some(output))
    }
}

/// 读取归档中的指定文件（便捷入口：每次重新解析文件表）。
///
/// 需要连续读取多个文件时请直接使用 [`ArchiveReader`]（只解析一次）。
pub fn read_file_from_archive(archive_bytes: &[u8], file_path: &str) -> Result<Option<Vec<u8>>> {
    ArchiveReader::new(archive_bytes)?.read(file_path)
}

/// 构建归档文件（归档头写入当前时间）
pub fn build_archive(files: &[(String, Vec<u8>, bool)]) -> Result<Vec<u8>> {
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    build_archive_with_time(files, created_at)
}

/// 以指定创建时间构建归档（**确定性入口**）
///
/// 归档头 `created_at` 是本函数唯一的墙钟字段。固定它之后，
/// 「相同文件列表 + 相同 `created_at` ⇒ 逐字节相同的归档」——
/// 供内容指纹（跨进程 hash 比对）场景使用，例如协作房间工程上传。
pub fn build_archive_with_time(
    files: &[(String, Vec<u8>, bool)],
    created_at: u64,
) -> Result<Vec<u8>> {
    let mut result = Vec::new();

    // 预留文件头空间
    let header_placeholder = [0u8; ArchiveHeader::SIZE];
    result.extend_from_slice(&header_placeholder);

    let mut entries = Vec::with_capacity(files.len());

    // 写入数据区
    for (path, data, should_compress) in files {
        let data_offset = result.len() as u64;

        let (stored_data, compressed_size, original_size, is_compressed) = if *should_compress {
            let compressed = zstd::stream::encode_all(std::io::Cursor::new(data), 3)
                .map_err(|e| CoreError::Compression(format!("archive compress: {e}")))?;
            let orig_len = data.len() as u64;
            let comp_len = compressed.len() as u64;
            (compressed, comp_len, orig_len, true)
        } else {
            let len = data.len() as u64;
            (data.clone(), len, len, false)
        };

        let crc32 = compute_crc32(&stored_data);
        result.extend_from_slice(&stored_data);

        entries.push(FileEntry {
            path: path.clone(),
            data_offset,
            compressed_size,
            original_size,
            crc32,
            is_compressed,
        });
    }

    // 构建并压缩文件表
    let file_table = FileTable { entries };
    let ft_encoded = file_table.encode();
    let ft_original_size = ft_encoded.len() as u64;
    let ft_compressed = zstd::stream::encode_all(std::io::Cursor::new(&ft_encoded), 3)
        .map_err(|e| CoreError::Compression(format!("file table compress: {e}")))?;
    let ft_compressed_size = ft_compressed.len() as u64;

    let file_table_offset = result.len() as u64;
    result.extend_from_slice(&ft_compressed);

    // 写入文件头
    let header = ArchiveHeader {
        magic: *b"LMPJ",
        version: 1,
        compression_flags: 0x01,
        file_table_offset,
        file_table_compressed_size: ft_compressed_size,
        file_table_original_size: ft_original_size,
        created_at,
        _reserved: [0u8; 16],
    };

    result[0..ArchiveHeader::SIZE].copy_from_slice(&header.to_bytes());

    Ok(result)
}

#[cfg(test)]
mod tests;
