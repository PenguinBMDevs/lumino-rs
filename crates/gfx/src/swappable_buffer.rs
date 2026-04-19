//! 双缓冲数据交换机制 - 实现 UI 线程和渲染线程的零拷贝数据共享
//!
//! 设计原理：
//! - Front Buffer: 渲染线程读取（只读）
//! - Back Buffer: UI 线程写入（独占写）
//! - 交换操作: 原子指针交换，无数据拷贝
//!
//! 使用场景：
//! - 百万级音符数据从 UI 线程传递到渲染线程
//! - 避免每帧的数据拷贝开销

use std::sync::Arc;
use std::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

/// 双缓冲结构
pub struct SwappableBuffer<T> {
    /// 前缓冲区指针（渲染线程读取）
    front: AtomicPtr<Vec<T>>,
    /// 后缓冲区指针（UI 线程写入）
    back: AtomicPtr<Vec<T>>,
    /// 数据版本号（用于同步检测）
    version: AtomicU64,
    /// 缓冲区 A（实际存储）
    buffer_a: Box<Vec<T>>,
    /// 缓冲区 B（实际存储）
    buffer_b: Box<Vec<T>>,
}

impl<T: Clone> SwappableBuffer<T> {
    /// 创建新的双缓冲
    pub fn new(initial_capacity: usize) -> Self {
        let mut buffer_a = Box::new(Vec::with_capacity(initial_capacity));
        let mut buffer_b = Box::new(Vec::with_capacity(initial_capacity));

        Self {
            front: AtomicPtr::new(buffer_a.as_mut() as *mut Vec<T>),
            back: AtomicPtr::new(buffer_b.as_mut() as *mut Vec<T>),
            version: AtomicU64::new(0),
            buffer_a,
            buffer_b,
        }
    }

    /// UI 线程：获取后缓冲区写入引用
    ///
    /// # Safety
    /// 必须在 UI 线程调用，且同一时间只能有一个写入者
    pub unsafe fn write_buffer(&self) -> &mut Vec<T> {
        let ptr = self.back.load(Ordering::Relaxed);
        unsafe { &mut *ptr }
    }

    /// UI 线程：提交写入并交换缓冲区
    ///
    /// 交换后，前缓冲区包含最新数据，渲染线程可以读取
    pub fn swap(&self) -> u64 {
        // 交换前后缓冲区指针
        let front_ptr = self.front.load(Ordering::Acquire);
        let back_ptr = self.back.load(Ordering::Acquire);

        self.front.store(back_ptr, Ordering::Release);
        self.back.store(front_ptr, Ordering::Release);

        // 递增版本号
        let new_version = self.version.fetch_add(1, Ordering::AcqRel) + 1;
        new_version
    }

    /// 渲染线程：获取前缓冲区读取引用
    ///
    /// # Safety
    /// 必须在渲染线程调用，且同一时间只能有一个读取者
    pub unsafe fn read_buffer(&self) -> &Vec<T> {
        let ptr = self.front.load(Ordering::Acquire);
        unsafe { &*ptr }
    }

    /// 获取当前版本号
    pub fn version(&self) -> u64 {
        self.version.load(Ordering::Acquire)
    }

    /// 检查是否有新数据（版本号变化）
    pub fn has_new_data(&self, last_version: u64) -> bool {
        self.version() != last_version
    }
}

impl<T> Drop for SwappableBuffer<T> {
    fn drop(&mut self) {
        // 将指针重置为空，避免 double free
        self.front.store(std::ptr::null_mut(), Ordering::Relaxed);
        self.back.store(std::ptr::null_mut(), Ordering::Relaxed);
    }
}

// 线程安全的 SwappableBuffer
pub type AtomicSwappableBuffer<T> = Arc<SwappableBuffer<T>>;

/// 渲染数据包 - 包含所有需要传递到渲染线程的数据
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
            let read_buf = buffer.read_buffer();
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
            let read_buf = buffer.read_buffer();
            assert_eq!(read_buf[0], 4);
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
