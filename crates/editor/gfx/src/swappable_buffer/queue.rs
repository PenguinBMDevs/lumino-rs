//! 跨线程数据传输类型
//!
//! 包含：
//! - `MpscQueue<T>`: 简化版多生产者单消费者队列（基于 Mutex + Condvar）
//! - `RenderData<T>`: 渲染线程每帧所需的数据结构

/// 渲染线程每帧所需的数据包
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
///
/// 使用 `Mutex<Option<T>>` + `Condvar` 实现。
/// 槽位满时丢弃新数据（非阻塞发送失败），接收端支持阻塞和非阻塞两种模式。
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
    /// 创建新的队列
    pub fn new() -> Self {
        Self {
            slot: std::sync::Mutex::new(None),
            signal: std::sync::Condvar::new(),
        }
    }

    /// 发送数据（非阻塞）
    ///
    /// 如果队列已有数据未被消费，返回 `Err(data)`。
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
    ///
    /// 如果没有数据，会阻塞直到有数据到达或锁被破坏。
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
    ///
    /// 如果没有数据，立即返回 `None`。
    pub fn try_recv(&self) -> Option<T> {
        let mut slot = self.slot.lock().ok()?;
        slot.take()
    }
}
