//! band 缓冲区槽位：task 期间 buffer 被 take 走，task 结束后归还。
//!
//! 槽位锁只在 take/put 时短暂持有，不跨越 async 执行体；任务 panic 导致
//! buffer 丢失时，下一次 `take` 返回默认 buffer（`ensure` 会重新分配），
//! 不会让整帧渲染永久失败。

use std::sync::Mutex;

use crate::buffer::BandBuffer;

/// 一个可复用的 band 槽位。
pub(crate) struct BandSlot {
    buffer: Mutex<Option<BandBuffer>>,
}

impl BandSlot {
    /// 创建带默认空 buffer 的槽位。
    pub(crate) fn new() -> Self {
        Self {
            buffer: Mutex::new(Some(BandBuffer::default())),
        }
    }
}

/// 从槽位取出 buffer（毒锁按内部值恢复；缺失时返回默认 buffer）。
pub(crate) fn take_buffer(slot: &BandSlot) -> BandBuffer {
    slot.buffer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
        .unwrap_or_default()
}

/// 把 buffer 归还槽位。
pub(crate) fn put_buffer(slot: &BandSlot, buffer: BandBuffer) {
    *slot
        .buffer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(buffer);
}
