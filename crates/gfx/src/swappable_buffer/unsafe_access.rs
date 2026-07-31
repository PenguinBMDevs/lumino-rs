//! 不安全操作合集
//!
//! 所有涉及 `UnsafeCell` 原始指针解引用的方法集中于此模块，
//! 便于审查和维护 Safety 前置条件。
//!
//! 三缓冲的核心安全保证：
//! - CAS 打包状态保证 writer/ready/reading 三者互异
//! - 写端与读端永远不会同时访问同一块物理内存
//! - UI 线程独占 writer 槽位，渲染线程独占 reading 槽位

use std::sync::atomic::Ordering;

use super::state::{pack_state, unpack_ready, unpack_reading, unpack_writer};
use super::SwappableBuffer;

impl<T> SwappableBuffer<T> {
    /// UI 线程：获取写入缓冲区引用（独占）
    ///
    /// # Safety
    ///
    /// 调用者必须保证：
    /// - 只能在 UI 线程调用，且同一时间只能有一个写入者
    ///   （SwappableBuffer 不内部同步写入访问，由外部协议保证）
    /// - 调用方在完成写入后**必须**调用 [`swap`](Self::swap) 提交数据，
    ///   否则数据不会到达渲染线程
    /// - 返回的 `&mut Vec<T>` 引用在调用 `swap` 或 `write_buffer` 再次获取之前
    ///   不得泄漏给其他线程
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn write_buffer(&self) -> &mut Vec<T> {
        let s = self.state.load(Ordering::Relaxed);
        let idx = unpack_writer(s);
        // Safety:
        // - `self.buffers` 在此方法调用期间不会被其他线程写入，
        //   因为调用者保证了唯一写入者（函数前置条件）
        // - `UnsafeCell::get()` 返回的裸指针在写入者独占期间是安全的
        // - `idx` 由打包状态中的 writer 索引解包得出，始终在 [0, 3) 范围内
        // - writer 槽位与 ready/reading 槽位在三缓冲协议下物理隔离，
        //   渲染线程不会同时访问此内存
        unsafe { &mut (*self.buffers.get())[idx] }
    }

    /// UI 线程：提交写入并交换缓冲
    ///
    /// CAS 交换 writer ↔ ready，保证三者互异的不变量。
    /// 调用前须先完成对 [`write_buffer`](Self::write_buffer) 返回缓冲的写入。
    ///
    /// 返回递增后的版本号。
    pub fn swap(&self) -> u64 {
        let mut s = self.state.load(Ordering::Acquire);
        loop {
            let w = unpack_writer(s);
            let r = unpack_ready(s);
            let rd = unpack_reading(s);
            // 交换 writer ↔ ready
            let new = pack_state(r as u8, w as u8, rd as u8);
            match self
                .state
                .compare_exchange_weak(s, new, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    // 发布刚提交的 ready 槽位统计（之前是 writer，写入已完成，cap/len 稳定）
                    // Safety:
                    // - CAS 已成功，此时 `w` 是旧 writer（即新 ready）索引
                    // - 写入者已完成对该槽位的写入，当前线程独占读取
                    // - `w` 由打包状态解包，保证在 [0, 3) 范围内
                    // - 只读访问 cap/len，不会导致数据竞争
                    let buf = unsafe { &(*self.buffers.get())[w] };
                    self.stats_cap[w].store(buf.capacity(), Ordering::Release);
                    self.stats_len[w].store(buf.len(), Ordering::Release);

                    // 发布新 writer 槽位统计（之前是 reading，渲染线程可能并发读取，
                    // 但均为只读访问，Vec 的 cap/len 字段在只读共享时不构成数据竞争）
                    // Safety:
                    // - CAS 成功后 `rd` 是旧 reading（即新 writer）索引
                    // - 渲染线程可能持有该槽位的 `&Vec<T>` 引用，但此处仅为只读访问
                    //   （读取 capacity() 和 len()，不写任何东西）
                    // - 多个读取者并发读取 `Vec` 的元数据是安全的
                    // - `rd` 由打包状态解包，保证在 [0, 3) 范围内
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
    ///
    /// 调用者必须保证：
    /// - 只能在渲染线程调用，且同一时间只能有一个读取者
    ///   （SwappableBuffer 不内部同步读取访问，由外部协议保证）
    /// - 调用方在拿到引用后到下一帧再次调用前，不得对缓冲区做写操作
    /// - 返回的 `&Vec<T>` 引用持有期间 UI 线程的 `swap()` 不会影响此槽位
    ///   （swap 只交换 writer↔ready，永不触及 reading）
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn acquire_read_buffer(&self) -> &Vec<T> {
        let mut s = self.state.load(Ordering::Acquire);
        loop {
            let r = unpack_ready(s);
            let rd = unpack_reading(s);
            // 交换 ready ↔ reading
            let new = pack_state((s & 0xFF) as u8, rd as u8, r as u8);
            match self
                .state
                .compare_exchange_weak(s, new, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => {
                    // 发布新 reading 槽位的统计（之前是 ready，未受写端影响，cap/len 稳定）
                    // 注意：此时 r 是旧 ready，在新状态中它变成了 reading
                    // Safety:
                    // - CAS 已成功，此时 `r` 是旧 ready（即新 reading）索引
                    // - write_buffer 在最坏情况下访问 writer 槽位，不碰此槽位
                    // - swap() 只交换 writer↔ready 不影响新 reading
                    // - `r` 由打包状态解包，保证在 [0, 3) 范围内
                    let buf = unsafe { &(*self.buffers.get())[r] };
                    self.reading_len.store(buf.len(), Ordering::Release);
                    self.stats_cap[r].store(buf.capacity(), Ordering::Release);
                    self.stats_len[r].store(buf.len(), Ordering::Release);

                    // Safety:
                    // - 同上的 CAS 已成功保证，返回的引用指向新 reading 槽位
                    // - 三缓冲协议保证此槽位不会再被 UI 线程写入（writer 槽位是另一个）
                    // - 调用者持有此引用期间仅做只读访问，符合 &Vec<T> 的借用约定
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
    ///
    /// 调用者必须保证：
    /// - 必须在 `acquire_read_buffer` 已在本帧被调用之后才能调用此方法
    /// - 此方法只读取 reading 槽位；`swap()` 只交换 writer↔ready 永不触及 reading，
    ///   因此并发 swap 下 peek 依然安全——无需额外保证"无并发 swap"
    #[allow(clippy::mut_from_ref)]
    pub unsafe fn peek_read_buffer(&self) -> &Vec<T> {
        let s = self.state.load(Ordering::Acquire);
        let rd = unpack_reading(s);
        // Safety:
        // - `rd` 是当前 reading 槽位索引，由打包状态解包，保证在 [0, 3) 范围内
        // - `acquire_read_buffer` 已在本帧被调用（函数前置条件），
        //   调用者持有该槽位的只读引用
        // - `swap()` 只交换 writer↔ready，永不触及 reading 槽位
        // - 此处只创建共享引用 `&Vec<T>`，与调用者已持有的引用一致，无数据竞争
        unsafe { &(*self.buffers.get())[rd] }
    }
}
