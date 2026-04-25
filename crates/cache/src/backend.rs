//! 跨平台 PageBackend 抽象层
//!
//! 提供统一的内存页访问接口：
//! - WindowsPageCache: 用户态页缓存（VirtualAlloc，64KB 页，显式 LRU + shrink）
//! - FileBackend: 直接从文件读取，不保留数据在内存（黑乐谱大文件场景）
//! - MemmapBackend (Linux/macOS): memmap2 全量 mmap

use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Mutex;

#[cfg(not(windows))]
pub mod memmap;
#[cfg(windows)]
pub mod windows;

#[cfg(not(windows))]
pub use memmap::MemmapBackend;
#[cfg(windows)]
pub use windows::WindowsPageCache;

/// 页后端统一接口
pub trait PageBackend: Send + Sync {
    fn read_exact(&self, offset: u64, buf: &mut [u8]) -> io::Result<()>;
    fn shrink(&mut self, max_bytes: u64);
    fn size(&self) -> u64;
    fn source_size(&self) -> u64;
}

/// 文件后端 — 从文件直接读取，不缓存数据在内存
///
/// 适用于黑乐谱超大文件场景。文件数据由 OS 页面缓存管理，
/// 进程 RSS 只包含当前热数据页。
pub struct FileBackend {
    file: Mutex<std::fs::File>,
    file_size: u64,
}

impl FileBackend {
    /// 打开现有文件作为后端
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = std::fs::File::open(path)?;
        let metadata = file.metadata()?;
        let file_size = metadata.len();
        Ok(Self {
            file: Mutex::new(file),
            file_size,
        })
    }

    /// 获取文件大小
    pub fn file_size(&self) -> u64 {
        self.file_size
    }
}

impl PageBackend for FileBackend {
    fn read_exact(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let end = offset.saturating_add(buf.len() as u64);
        if end > self.file_size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!(
                    "read_exact: offset {} + len {} exceeds file size {}",
                    offset,
                    buf.len(),
                    self.file_size
                ),
            ));
        }

        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(offset))?;
        file.read_exact(buf)
    }

    fn shrink(&mut self, _max_bytes: u64) {
        // 文件后端由 OS 管理页面缓存，不需显式 shrink
    }

    fn size(&self) -> u64 {
        self.file_size
    }

    fn source_size(&self) -> u64 {
        self.file_size
    }
}

/// 创建内存 PageBackend（小文件使用）
pub fn create_backend(data: Vec<u8>) -> Box<dyn PageBackend> {
    #[cfg(windows)]
    {
        Box::new(WindowsPageCache::new(data))
    }
    #[cfg(not(windows))]
    {
        Box::new(MemmapBackend::new(data))
    }
}

/// 创建文件 PageBackend（大文件使用，不缓存数据在内存）
pub fn create_file_backend(path: &Path) -> io::Result<Box<dyn PageBackend>> {
    Ok(Box::new(FileBackend::open(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_backend_roundtrip() {
        let data = b"Hello, PageBackend!".to_vec();
        let backend = create_backend(data.clone());
        let mut buf = vec![0u8; data.len()];
        backend.read_exact(0, &mut buf).unwrap();
        assert_eq!(buf, data);
        assert_eq!(backend.source_size(), data.len() as u64);
    }

    #[test]
    fn test_create_backend_partial() {
        let data = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ".to_vec();
        let backend = create_backend(data.clone());
        let mut buf = vec![0u8; 5];
        backend.read_exact(5, &mut buf).unwrap();
        assert_eq!(&buf, b"FGHIJ");
    }

    #[test]
    fn test_create_backend_end_of_data() {
        let data = b"short".to_vec();
        let backend = create_backend(data.clone());
        let mut buf = vec![0u8; 10];
        let result = backend.read_exact(0, &mut buf);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_backend_roundtrip() {
        use std::io::Write;
        let tmp_dir = std::env::temp_dir();
        let tmp_path = tmp_dir.join("lumino_cache_test_file_backend.bin");
        let mut file = std::fs::File::create(&tmp_path).unwrap();
        file.write_all(b"FileBackend test data!").unwrap();
        drop(file);

        let backend = FileBackend::open(&tmp_path).unwrap();
        assert_eq!(backend.source_size(), 22);
        let mut buf = vec![0u8; 7];
        backend.read_exact(0, &mut buf).unwrap();
        assert_eq!(&buf, b"FileBac");

        let _ = std::fs::remove_file(&tmp_path);
    }
}
