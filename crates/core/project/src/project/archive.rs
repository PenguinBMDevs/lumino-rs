//! LMPJ 归档文件格式定义与读写
//!
//! 单文件形态是文件夹形态的打包集合体，使用自定义轻量级归档格式。

use crc32fast::Hasher;

use lumino_core::error::{CoreError, Result};

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
        let mut pos = 2;

        if bytes.len() < pos + path_len + 8 + 8 + 8 + 4 + 1 {
            return Err(CoreError::FileFormat("file entry: incomplete".into()));
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
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 4 {
            return Err(CoreError::FileFormat("file table: too short".into()));
        }
        let count = read_u32_le(bytes, 0)? as usize;
        let mut entries = Vec::with_capacity(count);
        let mut pos = 4;

        for _ in 0..count {
            let (entry, consumed) = FileEntry::decode(&bytes[pos..])?;
            pos += consumed;
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

/// 读取归档中的指定文件
pub fn read_file_from_archive(archive_bytes: &[u8], file_path: &str) -> Result<Option<Vec<u8>>> {
    let header = ArchiveHeader::from_bytes(archive_bytes)?;
    if &header.magic != b"LMPJ" {
        return Err(CoreError::FileFormat("archive: invalid magic".into()));
    }

    let ft_start = header.file_table_offset as usize;
    let ft_end = ft_start + header.file_table_compressed_size as usize;
    let ft_data = &archive_bytes[ft_start..ft_end];

    let decompressed = zstd::stream::decode_all(std::io::Cursor::new(ft_data))
        .map_err(|e| CoreError::Compression(format!("file table decompression: {e}")))?;

    let file_table = FileTable::decode(&decompressed)?;

    let entry = file_table.entries.iter().find(|e| e.path == file_path);
    match entry {
        Some(e) => {
            let start = e.data_offset as usize;
            let end = start + e.compressed_size as usize;
            let data = &archive_bytes[start..end];

            if e.is_compressed {
                let decompressed = zstd::stream::decode_all(std::io::Cursor::new(data))
                    .map_err(|e| CoreError::Compression(format!("file decompression: {e}")))?;
                Ok(Some(decompressed))
            } else {
                Ok(Some(data.to_vec()))
            }
        }
        None => Ok(None),
    }
}

/// 构建归档文件
pub fn build_archive(files: &[(String, Vec<u8>, bool)]) -> Result<Vec<u8>> {
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
    let created_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
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
mod tests {
    use super::*;

    #[test]
    fn test_archive_header_roundtrip() {
        let header = ArchiveHeader {
            magic: *b"LMPJ",
            version: 1,
            compression_flags: 0x01,
            file_table_offset: 1024,
            file_table_compressed_size: 256,
            file_table_original_size: 512,
            created_at: 1716883200,
            _reserved: [0u8; 16],
        };
        let bytes = header.to_bytes();
        let decoded = ArchiveHeader::from_bytes(&bytes).expect("解码归档头部失败");
        assert_eq!(&decoded.magic, b"LMPJ");
        assert_eq!(decoded.version, 1);
        assert_eq!(decoded.file_table_offset, 1024);
    }

    #[test]
    fn test_file_entry_roundtrip() {
        let entry = FileEntry {
            path: "data/project/tracks/000.lmtrack".into(),
            data_offset: 55,
            compressed_size: 128,
            original_size: 256,
            crc32: 0xDEADBEEF,
            is_compressed: true,
        };
        let encoded = entry.encode();
        let (decoded, consumed) = FileEntry::decode(&encoded).expect("解码文件条目失败");
        assert_eq!(consumed, encoded.len());
        assert_eq!(decoded.path, entry.path);
        assert_eq!(decoded.data_offset, entry.data_offset);
        assert_eq!(decoded.crc32, entry.crc32);
        assert!(decoded.is_compressed);
    }

    #[test]
    fn test_build_and_read_archive() {
        let files = vec![
            ("metadata.toml".into(), b"name = \"Test\"".to_vec(), true),
            (
                "data/project/tracks/000.lmtrack".into(),
                vec![0x4C, 0x4D, 0x54, 0x52, 0x00, 0x01, 0x00, 0x00],
                true,
            ),
        ];

        let archive = build_archive(&files).expect("构建归档数据失败");
        assert!(!archive.is_empty());

        // 读取 metadata.toml
        let metadata =
            read_file_from_archive(&archive, "metadata.toml").expect("从归档读取metadata.toml失败");
        assert!(metadata.is_some());
        assert_eq!(
            metadata.expect("metadata.toml内容应为Some"),
            b"name = \"Test\""
        );

        // 读取不存在的文件
        let missing =
            read_file_from_archive(&archive, "notexist").expect("从归档读取不存在的文件失败");
        assert!(missing.is_none());
    }
}
