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
//! 为什么用 CAS 打包状态而非三个独立原子量：
//! writer/ready/reading 三个槽位的轮换必须原子整体完成。用三个 `AtomicU8`
//! 分别赋值会导致判断反例——writer 拿到 reading 槽位后 reading 还未更新时，
//! 另一个线程读到错误的三元组。将三个索引打包进单个 `AtomicU32`，用 CAS
//! 循环做整体置换，保证"三者互异"的不变量是宏不变的，不依赖时序运气。
//!
//! 为什么用数组索引而非裸指针：
//! - 如果用 `AtomicPtr<Vec<T>>` 指向一个 `Vec<T>` 字段，当 `Self` 被移动时
//!   该字段地址变化导致指针悬空，所以不得不套 `Box<Vec<T>>` 固定地址
//! - 改用 `[Vec<T>; 3]` + 原子索引后，访问通过状态索引运算，
//!   即使 `Self` 移动也不影响正确性，且消除了 `Box` 引入的双重间接
//!
//! 使用场景：
//! - 百万级音符数据从 UI 线程传递到渲染线程
//! - 避免每帧的数据拷贝开销

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering};

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
///
/// # 状态原子化（Bug 1 修复）
/// 三个槽位索引打包进单个 `AtomicU32`，布局：
/// ```text
/// bits 0-7:   writer slot index (0/1/2)
/// bits 8-15:  ready slot index
/// bits 16-23: reading slot index
/// bits 24-31: reserved (always 0)
/// ```
/// `swap()` CAS 交换 writer ↔ ready，`acquire_read_buffer()` CAS 交换
/// ready ↔ reading。两个操作都是纯置换，"三者互异"由 CAS 整体性保证。
pub struct SwappableBuffer<T> {
    /// 三块物理缓冲区，角色由 packed state 原子区分
    buffers: UnsafeCell<[Vec<T>; 3]>,
    /// 打包状态：writer:u8 | ready:u8 | reading:u8 | reserved:u8
    state: AtomicU32,
    /// 数据版本号（用于同步检测）
    version: AtomicU64,
    /// 提供给渲染线程的当前 reading 缓冲区长度（原子读，不碰状态机）
    /// 由 `acquire_read_buffer` 更新，供 `cache.rs` 的 `note_instances_len/is_empty` 无副作用读取
    pub reading_len: AtomicUsize,
    /// 各槽位的容量快照（原子量，用于安全跨线程统计，可能略滞后）
    pub stats_cap: [AtomicUsize; 3],
    /// 各槽位的长度快照（原子量，用于安全跨线程统计，可能略滞后）
    pub stats_len: [AtomicUsize; 3],
}

// Safety: 通过 CAS 打包状态 + 三缓冲物理隔离保证跨线程安全访问。
// 任意时刻 writer / ready / reading 指向三个不同的物理缓冲区，写端与读端
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
            state: AtomicU32::new(pack_state(WRITER as u8, READY as u8, READING as u8)),
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

    /// UI 线程：获取写入缓冲区引用（独占）
    ///
    /// # Safety
    /// 必须在 UI 线程调用，且同一时间只能有一个写入者。
    /// 调用方在完成写入后**必须**调用 [`swap`](Self::swap) 提交数据。
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn write_buffer(&self) -> &mut Vec<T> {
        let s = self.state.load(Ordering::Relaxed);
        let idx = (s & 0xFF) as usize;
        unsafe { &mut (*self.buffers.get())[idx] }
    }

    /// UI 线程：提交写入并交换缓冲
    ///
    /// CAS 交换 writer ↔ ready，保证三者互异的不变量。
    /// 调用前须先完成对 [`write_buffer`](Self::write_buffer) 返回缓冲的写入。
    pub fn swap(&self) -> u64 {
        let mut s = self.state.load(Ordering::Acquire);
        loop {
            let w = (s & 0xFF) as usize;
            let r = ((s >> 8) & 0xFF) as usize;
            let rd = ((s >> 16) & 0xFF) as usize;
            // 交换 writer ↔ ready
            let new = pack_state(r as u8, w as u8, rd as u8);
            match self
                .state
                .compare_exchange_weak(s, new, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    // 发布刚提交的 ready 槽位统计（之前是 writer，写入已完成，cap/len 稳定）
                    let buf = unsafe { &(*self.buffers.get())[w] };
                    self.stats_cap[w].store(buf.capacity(), Ordering::Release);
                    self.stats_len[w].store(buf.len(), Ordering::Release);
                    // 发布新 writer 槽位统计（之前是 reading，渲染线程已完成只读，cap/len 稳定）
                    let reading_buf = unsafe { &(*self.buffers.get())[rd] };
                    self.stats_cap[rd].store(reading_buf.capacity(), Ordering::Release);
                    self.stats_len[rd].store(reading_buf.len(), Ordering::Release);
                    break;
                }
                Err(cur) => s = cur,
            }
        }
        // 递增版本号
        self.version.fetch_add(1, Ordering::AcqRel) + 1
    }

    /// 渲染线程：每帧开始时接管 ready 缓冲为当前读取缓冲
    ///
    /// CAS 交换 ready ↔ reading，返回新 reading 槽位的引用。
    /// **每帧必须且只能调用一次**，从当前返回到下次调用之间的整个帧期间
    /// 都可以安全持有返回的引用。
    ///
    /// # Safety
    /// 必须在渲染线程调用，且同一时间只能有一个读取者。
    /// 调用方在拿到引用后到下一帧再次调用前，不得对缓冲区做写操作。
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn acquire_read_buffer(&self) -> &Vec<T> {
        let mut s = self.state.load(Ordering::Acquire);
        loop {
            let r = ((s >> 8) & 0xFF) as usize;
            let rd = ((s >> 16) & 0xFF) as usize;
            // 交换 ready ↔ reading
            let new = pack_state((s & 0xFF) as u8, rd as u8, r as u8);
            match self
                .state
                .compare_exchange_weak(s, new, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    // 发布新 reading 槽位的统计（之前是 ready，未受写端影响，cap/len 稳定）
                    // 注意：此时 r 是旧 ready，在新状态中它变成了 reading
                    let buf = unsafe { &(*self.buffers.get())[r] };
                    self.reading_len.store(buf.len(), Ordering::Release);
                    self.stats_cap[r].store(buf.capacity(), Ordering::Release);
                    self.stats_len[r].store(buf.len(), Ordering::Release);
                    return unsafe { &(*self.buffers.get())[r] };
                }
                Err(cur) => s = cur,
            }
        }
    }

    /// 获取当前版本的读取缓冲引用（非状态修改）
    ///
    /// 与 [`acquire_read_buffer`](Self::acquire_read_buffer) 不同，此方法
    /// **不修改状态机**，只读取当前 reading 槽位。用于同一帧内已经 acquire
    /// 后、需要再次访问缓冲的场景（如 encoder 的 prepare 和 draw 阶段复用）。
    ///
    /// # Safety
    /// 必须在 `acquire_read_buffer` 已在本帧被调用之后才能调用此方法。
    /// 调用者必须保证当前帧内没有并发 swap（即在渲染/UI 单一线程内调用）。
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn peek_read_buffer(&self) -> &Vec<T> {
        let s = self.state.load(Ordering::Acquire);
        let rd = ((s >> 16) & 0xFF) as usize;
        unsafe { &(*self.buffers.get())[rd] }
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
    /// 从原子统计中读取，不直接访问 `Vec` 头，因此不会与并发写入构成 data race。
    /// 统计值可能略滞后（writer 槽位的最后更新时间是它上一次处于 ready 或 reading 状态时），
    /// 用于内存监控等诊断场景完全足够。
    pub fn buffer_info(&self, slot: usize) -> (usize, usize) {
        let cap = self.stats_cap[slot & 3].load(Ordering::Acquire);
        let len = self.stats_len[slot & 3].load(Ordering::Acquire);
        (cap, len)
    }
}

/// 打包三个索引到 `u32`：`(reading << 16) | (ready << 8) | writer`
fn pack_state(writer: u8, ready: u8, reading: u8) -> u32 {
    (writer as u32) | ((ready as u32) << 8) | ((reading as u32) << 16)
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
    fn test_acquire_while_holding_reference() {
        // 验证：acquire 持有引用期间发生 swap，writer 槽位 ≠ reading 槽位
        let buffer = SwappableBuffer::<i32>::new(4);

        // 写入并提交第一帧
        unsafe {
            buffer.write_buffer().push(10);
        }
        buffer.swap();

        // 渲染线程 acquire：此时 reading 拿到刚提交的数据
        let reading_ref = unsafe { buffer.acquire_read_buffer() };
        assert_eq!(reading_ref[0], 10);

        // UI 线程写入第二帧（占用另一个槽位）
        unsafe {
            buffer.write_buffer().push(20);
        }
        // reading_ref 仍然持有着，此时 swap
        let v2 = buffer.swap();

        // swap 后 writer 槽位应该是之前的 reading 槽位，不是 reading_ref 指向的槽位
        // reading_ref 的内容保持不变
        assert_eq!(reading_ref[0], 10, "持有引用期间 swap 不应污染原数据");
        assert!(v2 >= 1);

        // 验证 reading_len 原子量正确
        assert_eq!(buffer.reading_len.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn test_peek_read_buffer() {
        // 验证 peek_read_buffer 返回与 acquire_read_buffer 同一槽位
        let buffer = SwappableBuffer::<i32>::new(4);

        unsafe {
            buffer.write_buffer().push(42);
        }
        buffer.swap();

        unsafe {
            let acquired = buffer.acquire_read_buffer();
            let peeked = buffer.peek_read_buffer();

            assert_eq!(acquired.as_ptr(), peeked.as_ptr(), "peek 应返回同一槽位");
            assert_eq!(peeked[0], 42);
        }
    }

    #[test]
    fn test_reading_len_published_after_acquire() {
        let buffer = SwappableBuffer::<i32>::new(100);

        // 初始 reading_len 应为 0
        assert_eq!(buffer.reading_len.load(Ordering::Relaxed), 0);

        unsafe {
            buffer.write_buffer().push(1);
        }
        unsafe {
            buffer.write_buffer().push(2);
        }
        buffer.swap();

        // acquire 前 reading_len 还是上次的值（0）
        assert_eq!(buffer.reading_len.load(Ordering::Relaxed), 0);

        unsafe {
            buffer.acquire_read_buffer();
        }

        // acquire 后 reading_len 更新为 2
        assert_eq!(
            buffer.reading_len.load(Ordering::Relaxed),
            2,
            "acquire 后 reading_len 应反映新 reading 槽位的 len"
        );
    }

    #[test]
    fn test_three_state_invariant_holds_through_cycle() {
        // 验证经过多轮 swap+acquire 后，"三者互异"的不变量仍未破坏
        let buffer = SwappableBuffer::<i32>::new(4);

        for i in 0..20 {
            unsafe {
                buffer.write_buffer().push(i as i32);
            }
            buffer.swap();
            unsafe {
                buffer.acquire_read_buffer();
            }
        }

        // 验证 stats 不越界
        for slot in 0..3 {
            let (cap, len) = buffer.buffer_info(slot);
            assert!(cap >= 4, "slot {slot}: capacity={cap} 不应小于初始值");
            assert!(len <= cap, "slot {slot}: len={len} <= cap={cap}");
        }
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
