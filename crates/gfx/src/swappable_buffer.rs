//! 三缓冲数据交换机制 - UI 线程和渲染线程的零拷贝数据共享
//!
//! writer/ready/reading 三者始终指向三个不同的物理缓冲区，
//! 写端与读端永不访问同一块内存，从根上消除数据竞争。

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize};

pub(crate) mod state;
mod unsafe_access;
mod queue;
#[cfg(test)]
mod tests;
pub use queue::{MpscQueue, RenderData};

/// 三缓冲结构
///
/// 状态机（始终保证三个角色映射到三个不同槽位）：
/// - `writer`: UI 线程独占写入的槽位（WRITER）
/// - `ready`: 最近一次 swap 提交的、等待渲染接管的槽位（READY）
/// - `reading`: 渲染线程当前持有只读引用的槽位（READING）
pub struct SwappableBuffer<T> {
    /// 三块物理缓冲区，角色由 packed state 原子区分
    buffers: UnsafeCell<[Vec<T>; 3]>,
    /// 打包状态：writer:u8 | ready:u8 | reading:u8 | reserved:u8
    state: AtomicU32,
    /// 数据版本号（用于同步检测）
    version: AtomicU64,
    /// 提供给渲染线程的当前 reading 缓冲区长度（原子读，不碰状态机）
    pub reading_len: AtomicUsize,
    /// 各槽位的容量快照（原子量，用于安全跨线程统计）
    pub stats_cap: [AtomicUsize; 3],
    /// 各槽位的长度快照（原子量，用于安全跨线程统计）
    pub stats_len: [AtomicUsize; 3],
}
// Safety: 通过 CAS 打包状态 + 三缓冲物理隔离保证跨线程安全访问。
// 任意时刻 writer/ready/reading 指向三个不同的物理缓冲区，写端与读端
// 永远不会同时访问同一块内存，因此不存在数据竞争。
unsafe impl<T: Send> Sync for SwappableBuffer<T> {}

impl<T> SwappableBuffer<T> {
    /// 创建新的三缓冲
    pub fn new(initial_capacity: usize) -> Self {
        let cap = if initial_capacity > 0 {
            initial_capacity
        } else {
            4 // 最小容量，避免零容量 Vec 后续反复扩容
        };
        Self {
            buffers: UnsafeCell::new([
                Vec::with_capacity(cap),
                Vec::with_capacity(cap),
                Vec::with_capacity(cap),
            ]),
            // 初始: writer=0, ready=1, reading=2
            state: AtomicU32::new(state::pack_state(
                state::WRITER as u8,
                state::READY as u8,
                state::READING as u8,
            )),
            version: AtomicU64::new(0),
            reading_len: AtomicUsize::new(0),
            stats_cap: [
                AtomicUsize::new(cap),
                AtomicUsize::new(cap),
                AtomicUsize::new(cap),
            ],
            stats_len: [
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
            ],
        }
    }

    /// 获取当前版本号
    pub fn version(&self) -> u64 {
        self.version.load(std::sync::atomic::Ordering::Acquire)
    }

    /// 检查是否有新数据（版本号变化）
    pub fn has_new_data(&self, last_version: u64) -> bool {
        self.version() != last_version
    }

    /// 获取指定物理槽位（0=writer, 1=ready, 2=reading）的容量与长度快照。
    ///
    /// 从原子统计中读取，不直接访问 `Vec` 头，因此不会与并发写入构成 data race。
    pub fn buffer_info(&self, slot: usize) -> (usize, usize) {
        let cap = self.stats_cap[slot & 3].load(std::sync::atomic::Ordering::Acquire);
        let len = self.stats_len[slot & 3].load(std::sync::atomic::Ordering::Acquire);
        (cap, len)
    }
}

/// 线程安全的 SwappableBuffer（Arc 包装）
pub type AtomicSwappableBuffer<T> = Arc<SwappableBuffer<T>>;
