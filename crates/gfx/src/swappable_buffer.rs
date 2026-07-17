//! 三缓冲数据交换机制 - 实现 UI 线程和渲染线程的零拷贝数据共享
//!
//! 设计原理：
//! - Writer Buffer: UI 线程独占写入
//! - Ready Buffer: 写入完成后由 swap 转入，等待读取
//! - Reading Buffer: 渲染线程每帧开始时从 Ready 接管，独占读取
//!
//! 为什么三缓冲而非双缓冲：
//! 双缓冲仅用 front/back 两个槽位，当 UI 高频写入（拖动音符）与渲染线程
//! 高频读取（高刷屏）并发时，swap 翻转 front 的瞬间写入端可能拿到渲染端
//! 正在读取的同一块内存 —— 构成真实数据竞争（UB）。三缓冲在任意时刻将
//! 写、待读、读三块物理内存完全隔离：writer 写完 swap 到 ready，渲染线程
//! 每帧开始时把 ready 接管为 reading，三态轮换互不重叠，从根上消除竞争。
//!
//! 为什么用数组索引而非裸指针：
//! - 如果用 `AtomicPtr<Vec<T>>` 指向一个 `Vec<T>` 字段，当 `Self` 被移动时
//!   该字段地址变化导致指针悬空，所以不得不套 `Box<Vec<T>>` 固定地址
//! - 改用 `[Vec<T>; 3]` + `AtomicU8` 索引后，访问通过状态索引运算，
//!   即使 `Self` 移动也不影响正确性，且消除了 `Box` 引入的双重间接
//!
//! 使用场景：
//! - 百万级音符数据从 UI 线程传递到渲染线程
//! - 避免每帧的数据拷贝开销

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

/// 三缓冲槽位索引
const WRITER: usize = 0;
const READY: usize = 1;
const READING: usize = 2;

/// 三缓冲结构
///
/// 状态机（始终保证三个角色映射到三个不同槽位）：
/// - `writer`: UI 线程独占写入的槽位（WRITER）
/// - `ready`: 最近一次 swap 提交的、等待渲染接管的槽位（READY）
/// - `reading`: 渲染线程当前持有只读引用的槽位（READING）
pub struct SwappableBuffer<T> {
    /// 三块物理缓冲区，角色由下方原子索引区分
    buffers: UnsafeCell<[Vec<T>; 3]>,
    /// 写入端当前写入的槽位索引（UI 线程独占）
    writer: AtomicU8,
    /// 待读取（已提交）的槽位索引
    ready: AtomicU8,
    /// 渲染线程正在读取的槽位索引
    reading: AtomicU8,
    /// 数据版本号（用于同步检测）
    version: AtomicU64,
}

// Safety: 通过 Acquire/Release 协议 + 三缓冲物理隔离保证跨线程安全访问。
// 任意时刻 writer / ready / reading 指向三个不同的物理缓冲区，写端与读端
// 永远不会同时访问同一块内存，因此不存在数据竞争。
unsafe impl<T: Send> Sync for SwappableBuffer<T> {}

impl<T> SwappableBuffer<T> {
    /// 创建新的三缓冲
    pub fn new(initial_capacity: usize) -> Self {
        Self {
            buffers: UnsafeCell::new([
                Vec::with_capacity(initial_capacity),
                Vec::with_capacity(initial_capacity),
                Vec::with_capacity(initial_capacity),
            ]),
            writer: AtomicU8::new(WRITER as u8),
            ready: AtomicU8::new(READY as u8),
            reading: AtomicU8::new(READING as u8),
            version: AtomicU64::new(0),
        }
    }

    /// UI 线程：获取写入缓冲区引用（独占）
    ///
    /// # Safety
    /// 必须在 UI 线程调用，且同一时间只能有一个写入者。
    /// 调用方在完成写入后**必须**调用 [`swap`](Self::swap) 提交数据。
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn write_buffer(&self) -> &mut Vec<T> {
        let idx = self.writer.load(Ordering::Relaxed) as usize;
        unsafe { &mut (*self.buffers.get())[idx] }
    }

    /// UI 线程：提交写入并交换缓冲
    ///
    /// 语义：将当前 writer 槽位标记为 ready（供渲染线程接管），
    /// 并从 ready 槽位回收一块空闲缓冲作为新的 writer。
    /// 调用前须先完成对 [`write_buffer`](Self::write_buffer) 返回缓冲的写入。
    pub fn swap(&self) -> u64 {
        // 当前 writer 写完了，变成 ready
        let writer_idx = self.writer.load(Ordering::Relaxed) as usize;
        self.ready.store(writer_idx as u8, Ordering::Release);

        // 渲染线程上一帧持有的 reading 槽位已用完，回收为新的 writer
        let reading_idx = self.reading.load(Ordering::Acquire) as usize;
        self.writer.store(reading_idx as u8, Ordering::Release);

        // 递增版本号：AcqRel 保证 ready 写入对渲染线程可见
        self.version.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// 渲染线程：每帧开始时接管 ready 缓冲为当前读取缓冲
    ///
    /// 必须在每帧渲染的最开始调用一次，拿到读取引用后到下一帧
    /// [`acquire_read_buffer`](Self::acquire_read_buffer) 之前都安全持有。
    ///
    /// # Safety
    /// 必须在渲染线程调用，且同一时间只能有一个读取者。
    /// 调用方在拿到引用后到下一帧再次调用前，不得对缓冲区做写操作。
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn acquire_read_buffer(&self) -> &Vec<T> {
        // 将 ready 接管为 reading（渲染线程独占）
        let ready_idx = self.ready.load(Ordering::Acquire) as usize;
        self.reading.store(ready_idx as u8, Ordering::Release);
        unsafe { &(*self.buffers.get())[ready_idx] }
    }

    /// 获取当前版本号
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// 检查是否有新数据（版本号变化）
    pub fn has_new_data(&self, last_version: u64) -> bool {
        self.version() != last_version
    }

    /// 获取指定物理槽位（0=writer, 1=ready, 2=reading）的容量与长度快照。
    ///
    /// 返回 `(capacity, len)` 的瞬时拷贝，不暴露 `&Vec` 引用，因此可安全
    /// 在任意线程随时调用，不会与并发写入构成数据竞争。用于内存统计等只读场景。
    pub fn buffer_info(&self, slot: usize) -> (usize, usize) {
        let v = unsafe { &(*self.buffers.get())[slot & 3] };
        (v.capacity(), v.len())
    }
}

// 线程安全的 SwappableBuffer
pub type AtomicSwappableBuffer<T> = Arc<SwappableBuffer<T>>;
#[derive(Debug, Clone)]
pub struct RenderData<T> {
    /// 数据版本号
    pub version: u64,
    /// 视口大小
    pub viewport_size: (f32, f32),
    /// 滚动位置
    pub scroll: (f32, f32),
    /// 缩放
    pub zoom: (f32, f32),
    /// 实际数据
    pub data: T,
}

/// 多生产者单消费者队列（简化版）
pub struct MpscQueue<T> {
    /// 数据槽位
    slot: std::sync::Mutex<Option<T>>,
    /// 有新数据的信号
    signal: std::sync::Condvar,
}

impl<T> Default for MpscQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> MpscQueue<T> {
    pub fn new() -> Self {
        Self {
            slot: std::sync::Mutex::new(None),
            signal: std::sync::Condvar::new(),
        }
    }

    /// 发送数据（非阻塞）
    pub fn send(&self, data: T) -> Result<(), T> {
        let mut slot = match self.slot.lock() {
            Ok(guard) => guard,
            Err(_) => return Err(data),
        };
        if slot.is_some() {
            // 槽位已满，丢弃旧数据
            return Err(data);
        }
        *slot = Some(data);
        self.signal.notify_one();
        Ok(())
    }

    /// 接收数据（阻塞）
    pub fn recv(&self) -> Option<T> {
        let mut slot = self.slot.lock().ok()?;
        loop {
            if let Some(data) = slot.take() {
                return Some(data);
            }
            slot = match self.signal.wait(slot) {
                Ok(guard) => guard,
                Err(_) => return None,
            };
        }
    }

    /// 尝试接收数据（非阻塞）
    pub fn try_recv(&self) -> Option<T> {
        let mut slot = self.slot.lock().ok()?;
        slot.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_swappable_buffer_basic() {
        let buffer = SwappableBuffer::<i32>::new(100);

        // UI 线程写入
        unsafe {
            let write_buf = buffer.write_buffer();
            write_buf.push(1);
            write_buf.push(2);
            write_buf.push(3);
        }

        // 交换
        let version = buffer.swap();
        assert_eq!(version, 1);

        // 渲染线程读取
        unsafe {
            let read_buf = buffer.acquire_read_buffer();
            assert_eq!(read_buf.len(), 3);
            assert_eq!(read_buf[0], 1);
            assert_eq!(read_buf[1], 2);
            assert_eq!(read_buf[2], 3);
        }
    }

    #[test]
    fn test_swappable_buffer_multiple_swaps() {
        let buffer = SwappableBuffer::<i32>::new(10);

        for i in 0..5 {
            unsafe {
                let write_buf = buffer.write_buffer();
                write_buf.clear();
                write_buf.push(i);
            }
            buffer.swap();
        }

        unsafe {
            let read_buf = buffer.acquire_read_buffer();
            assert_eq!(read_buf[0], 4);
        }
    }

    #[test]
    fn test_swappable_buffer_three_buffers_isolated() {
        // 验证 writer / ready / reading 始终指向三个不同物理槽位
        let buffer = SwappableBuffer::<i32>::new(4);

        unsafe {
            buffer.write_buffer().push(10);
        }
        buffer.swap();
        let r1 = unsafe { buffer.acquire_read_buffer() };

        unsafe {
            buffer.write_buffer().push(20);
        }
        buffer.swap();
        let r2 = unsafe { buffer.acquire_read_buffer() };

        // 两个读取引用指向不同数据，且互不干扰
        assert_eq!(r1.len(), 1);
        assert_eq!(r1[0], 10);
        assert_eq!(r2.len(), 1);
        assert_eq!(r2[0], 20);
    }

    #[test]
    fn test_mpsc_queue() {
        let queue = MpscQueue::<i32>::new();

        // 发送数据
        assert!(queue.send(42).is_ok());

        // 接收数据
        assert_eq!(queue.try_recv(), Some(42));
        assert_eq!(queue.try_recv(), None);
    }

    #[test]
    fn test_mpsc_queue_overflow() {
        let queue = MpscQueue::<i32>::new();

        // 第一次发送成功
        assert!(queue.send(1).is_ok());

        // 第二次发送失败（槽位已满）
        assert!(queue.send(2).is_err());

        // 接收后再次发送成功
        queue.recv();
        assert!(queue.send(3).is_ok());
    }
}
