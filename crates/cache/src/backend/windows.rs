//! Windows 用户态页缓存后端
//!
//! 使用 `VirtualAlloc` 分配 64 KB 页面，LRU 淘汰，支持显式 shrink。
//! 避免使用 mmap（黑乐谱超大文件会导致 Windows 虚地址空间虚高）。
//!
//! 内存约束：
//! - 默认 max_pages 按系统可用内存 10% 计算（最少 512 页 = 32 MB）
//! - 每页 64 KB，512 页 ≈ 32 MB，4096 页 ≈ 256 MB

use std::collections::HashMap;
use std::io;
use std::sync::Mutex;

use crate::params;

use winapi::ctypes::c_void;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::memoryapi::VirtualAlloc;
use winapi::um::memoryapi::VirtualFree;
use winapi::um::sysinfoapi::GlobalMemoryStatusEx;
use winapi::um::sysinfoapi::MEMORYSTATUSEX;
use winapi::um::winnt::MEM_COMMIT;
use winapi::um::winnt::MEM_RELEASE;
use winapi::um::winnt::MEM_RESERVE;
use winapi::um::winnt::PAGE_READWRITE;

/// 计算建议的页缓存上限
///
/// 基于系统可用物理内存的 10%，至少 `DEFAULT_MAX_PAGES` 页。
fn suggested_max_pages() -> usize {
    let mut mem_status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    mem_status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;

    let result = unsafe { GlobalMemoryStatusEx(&mut mem_status) };
    let avail_bytes = if result != 0 {
        mem_status.ullAvailPhys as usize
    } else {
        // 回退：默认 1 GB
        1024 * 1024 * 1024
    };

    let target_bytes = (avail_bytes as f64 * params::SYSTEM_MEMORY_PERCENT) as usize;
    let pages = target_bytes / params::WINDOWS_PAGE_SIZE;
    pages.max(params::DEFAULT_MAX_PAGES)
}

/// 单页数据 — 使用 VirtualAlloc 分配的 64 KB 页
// SAFETY: `ptr` 指向 VirtualAlloc 分配的页，仅通过 `Page` 的方法访问，
// Mutex 保证线程安全。
unsafe impl Send for Page {}

struct Page {
    ptr: *mut u8,
    capacity: usize,
    offset: u64,
}

impl Page {
    fn new() -> Self {
        let ptr = unsafe {
            VirtualAlloc(
                std::ptr::null_mut(),
                params::WINDOWS_PAGE_SIZE,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };

        if ptr.is_null() {
            let err = unsafe { GetLastError() };
            panic!(
                "WindowsPageCache: VirtualAlloc failed for {} bytes (error {})",
                params::WINDOWS_PAGE_SIZE,
                err
            );
        }

        Self {
            ptr: ptr as *mut u8,
            capacity: params::WINDOWS_PAGE_SIZE,
            offset: u64::MAX,
        }
    }

    /// 获取页内数据的可变切片
    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.capacity) }
    }

    /// 获取页内数据的切片
    fn as_slice(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.ptr, self.capacity) }
    }

    fn free(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                VirtualFree(self.ptr as *mut c_void, self.capacity, MEM_RELEASE);
            }
            self.ptr = std::ptr::null_mut();
            self.capacity = 0;
        }
    }
}

impl Drop for Page {
    fn drop(&mut self) {
        self.free();
    }
}

/// 内部可变状态
struct Inner {
    pages: HashMap<u64, Page>,
    lru_list: Vec<u64>,
    source_data: Vec<u8>,
    source_size: u64,
    max_pages: usize,
}

/// Windows 用户态页缓存
///
/// 将源数据分页缓存，每页 64 KB，LRU 淘汰。
/// 使用 `VirtualAlloc` 分配避免 C 运行时堆碎片化。
pub struct WindowsPageCache {
    inner: Mutex<Inner>,
}

impl WindowsPageCache {
    /// 创建新的 Windows 页缓存
    pub fn new(source_data: Vec<u8>) -> Self {
        let source_size = source_data.len() as u64;
        let max_pages = suggested_max_pages();

        let inner = Inner {
            pages: HashMap::new(),
            lru_list: Vec::with_capacity(max_pages),
            source_data,
            source_size,
            max_pages,
        };

        Self {
            inner: Mutex::new(inner),
        }
    }

    /// 获取当前缓存页数
    pub fn page_count(&self) -> usize {
        self.inner.lock().unwrap().pages.len()
    }

    fn ensure_page(inner: &mut Inner, file_offset: u64) -> io::Result<&mut Page> {
        let page_start = file_offset & !(params::WINDOWS_PAGE_SIZE as u64 - 1);

        if !inner.pages.contains_key(&page_start) {
            // 淘汰旧页
            while inner.pages.len() >= inner.max_pages && !inner.lru_list.is_empty() {
                let oldest = inner.lru_list.remove(0);
                let mut page = inner
                    .pages
                    .remove(&oldest)
                    .expect("LRU 列表与 pages 不一致");
                page.free();
            }

            // 从源数据加载页
            let mut page = Page::new();
            let copy_start = page_start as usize;
            let copy_end = (copy_start + params::WINDOWS_PAGE_SIZE).min(inner.source_data.len());
            let copy_len = copy_end - copy_start;

            if copy_len > 0 {
                let dst = page.as_mut_slice();
                dst[..copy_len].copy_from_slice(&inner.source_data[copy_start..copy_end]);
            }

            page.offset = page_start;
            inner.pages.insert(page_start, page);
        }

        // 更新 LRU
        if let Some(pos) = inner.lru_list.iter().position(|&k| k == page_start) {
            inner.lru_list.remove(pos);
        }
        inner.lru_list.push(page_start);

        Ok(inner.pages.get_mut(&page_start).unwrap())
    }
}

impl super::PageBackend for WindowsPageCache {
    fn read_exact(&self, offset: u64, buf: &mut [u8]) -> io::Result<()> {
        let source_size = self.inner.lock().unwrap().source_size;
        if offset.saturating_add(buf.len() as u64) > source_size {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "read_exact: offset + len exceeds source size",
            ));
        }

        let mut inner = self.inner.lock().unwrap();
        let mut remaining = buf.len();
        let mut cur_offset = offset;
        let mut buf_pos = 0;

        while remaining > 0 {
            let page = Self::ensure_page(&mut inner, cur_offset)?;
            let page_offset_in_source = page.offset;
            let page_local_offset = (cur_offset - page_offset_in_source) as usize;
            let copy_len = remaining.min(params::WINDOWS_PAGE_SIZE - page_local_offset);

            let src = page.as_slice();
            buf[buf_pos..buf_pos + copy_len]
                .copy_from_slice(&src[page_local_offset..page_local_offset + copy_len]);

            remaining -= copy_len;
            cur_offset += copy_len as u64;
            buf_pos += copy_len;
        }

        Ok(())
    }

    fn shrink(&mut self, max_bytes: u64) {
        let max_pages = (max_bytes / params::WINDOWS_PAGE_SIZE as u64) as usize;
        let max_pages = max_pages.max(16); // 至少保留 16 页

        let mut inner = self.inner.lock().unwrap();

        // 更新 max_pages — shrink 后新分配受此限制
        inner.max_pages = max_pages;

        // 淘汰超过上限的页
        while inner.pages.len() > max_pages && !inner.lru_list.is_empty() {
            let oldest = inner.lru_list.remove(0);
            if let Some(mut page) = inner.pages.remove(&oldest) {
                page.free();
            }
        }
    }

    fn size(&self) -> u64 {
        let inner = self.inner.lock().unwrap();
        inner.pages.len() as u64 * params::WINDOWS_PAGE_SIZE as u64
    }

    fn source_size(&self) -> u64 {
        self.inner.lock().unwrap().source_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::PageBackend;

    fn make_data(size: usize) -> Vec<u8> {
        (0..size).map(|i| (i % 256) as u8).collect()
    }

    #[test]
    fn test_windows_cache_small_data() {
        let data = make_data(1000);
        let cache = WindowsPageCache::new(data.clone());
        let mut buf = vec![0u8; 100];
        cache.read_exact(50, &mut buf).unwrap();

        let expected: Vec<u8> = (50..150).map(|i| (i % 256) as u8).collect();
        assert_eq!(buf, expected);
    }

    #[test]
    fn test_windows_cache_cross_page() {
        let data = make_data(params::WINDOWS_PAGE_SIZE + 100);
        let cache = WindowsPageCache::new(data.clone());
        let start_offset = (params::WINDOWS_PAGE_SIZE - 10) as u64;
        let mut buf = vec![0u8; 20];
        cache.read_exact(start_offset, &mut buf).unwrap();

        assert_eq!(
            buf[..10],
            data[start_offset as usize..start_offset as usize + 10]
        );
        assert_eq!(
            buf[10..],
            data[start_offset as usize + 10..start_offset as usize + 20]
        );
    }

    #[test]
    fn test_windows_cache_shrink() {
        let data = make_data(params::WINDOWS_PAGE_SIZE * 10);
        let mut cache = WindowsPageCache::new(data);
        assert!(cache.page_count() == 0);
        cache.shrink(params::WINDOWS_PAGE_SIZE as u64 * 2);
        // shrink before any reads should work fine
    }

    #[test]
    fn test_windows_cache_eof_error() {
        let data = make_data(100);
        let cache = WindowsPageCache::new(data);
        let mut buf = vec![0u8; 10];
        let result = cache.read_exact(95, &mut buf);
        assert!(result.is_err());
    }
}
