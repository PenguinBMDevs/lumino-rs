//! Linux/macOS memmap2 后端
//!
//! 使用 `memmap2` crate 进行全量内存映射。
//! 可执行文件大小的 .mid 文件在此平台上由操作系统管理缺页中断，
//! Rust 端不需要手动分页管理。
//! 对于 POSIX 系统，这是最有效的随机访问方式。

use std::fs::File;
use std::io::{self, Write};
use std::path::Path;

use memmap2::Mmap;

/// memmap2 后端
///
/// 将源数据写入临时文件后进行 mmap。
/// `shrink()` 为空操作——操作系统管理映射页的换入换出。
/// `size()` 返回映射总大小（mmap 的虚拟地址空间）。
pub struct MemmapBackend {
    /// 内存映射
    mmap: Mmap,
    /// 映射大小
    size: u64,
    /// 临时文件路径（调试用）
    _temp_path: Option<std::path::PathBuf>,
}

impl MemmapBackend {
    /// 创建新的 mmap 后端
    ///
    /// 将数据写入临时文件后进行 mmap。
    /// 临时文件在 Drop 时自动清理。
    pub fn new(source_data: Vec<u8>) -> Self {
        let size = source_data.len() as u64;

        // 创建临时文件并写入数据
        let (file, temp_path) =
            create_temp_file(&source_data).expect("MemmapBackend: 创建临时文件失败");

        let mmap = unsafe { Mmap::map(&file).expect("MemmapBackend: mmap 失败") };

        // 关闭文件描述符（mmap 保持引用）
        drop(file);

        Self {
            mmap,
            size,
            _temp_path: temp_path,
        }
    }

    /// 从现有文件创建 mmap 后端
    ///
    /// 用于测试和直接文件映射场景。
    pub fn from_file<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path.as_ref())?;
        let metadata = file.metadata()?;
        let size = metadata.len();

        let mmap = unsafe { Mmap::map(&file)? };

        Ok(Self {
            mmap,
            size,
            _temp_path: None,
        })
    }
}

/// 创建临时文件并写入数据
fn create_temp_file(data: &[u8]) -> io::Result<(File, Option<std::path::PathBuf>)> {
    let mut temp_path = std::env::temp_dir();
    temp_path.push(format!("lumino_cache_{:016x}", rand_fallback()));

    let mut file = File::create(&temp_path)?;
    file.write_all(data)?;
    file.sync_all()?;
    file.seek(std::io::SeekFrom::Start(0))?;

    Ok((file, Some(temp_path)))
}

/// 回退随机数（不依赖外部 crate）
fn rand_fallback() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let nanos = now.as_nanos();
    // 混合时间戳和地址随机性
    (nanos as u64) ^ (nanos >> 32) as u64
}

impl Drop for MemmapBackend {
    fn drop(&mut self) {
        // 清理临时文件
        if let Some(path) = &self._temp_path {
            let _ = std::fs::remove_file(path);
        }
    }
}

impl super::PageBackend for MemmapBackend {
    fn read_exact(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let end = offset.saturating_add(buf.len() as u64);
        if end > self.size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "read_exact: offset {} + len {} exceeds source size {}",
                    offset,
                    buf.len(),
                    self.size
                ),
            ));
        }

        let start = offset as usize;
        let end = end as usize;
        buf.copy_from_slice(&self.mmap[start..end]);
        Ok(())
    }

    fn shrink(&mut self, _max_bytes: u64) {
        // mmap 由操作系统管理换页，Rust 端不需要手动 shrink
        tracing::trace!("MemmapBackend::shrink is a no-op on this platform");
    }

    fn size(&self) -> u64 {
        self.size
    }

    fn source_size(&self) -> u64 {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::PageBackend;

    #[test]
    fn test_memmap_roundtrip() {
        let data = b"MemmapBackend test data!".to_vec();
        let backend = MemmapBackend::new(data.clone());
        let mut buf = vec![0u8; data.len()];
        backend.read_exact(0, &mut buf).unwrap();
        assert_eq!(buf, data);
    }

    #[test]
    fn test_memmap_partial_read() {
        let data = b"Hello, memmap world!".to_vec();
        let backend = MemmapBackend::new(data);
        let mut buf = vec![0u8; 7];
        backend.read_exact(0, &mut buf).unwrap();
        assert_eq!(&buf, b"Hello, ");
    }

    #[test]
    fn test_memmap_eof() {
        let data = b"short".to_vec();
        let backend = MemmapBackend::new(data);
        let mut buf = vec![0u8; 10];
        let result = backend.read_exact(0, &mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_memmap_source_size() {
        let data = vec![0u8; 12345];
        let backend = MemmapBackend::new(data);
        assert_eq!(backend.source_size(), 12345);
    }
}
