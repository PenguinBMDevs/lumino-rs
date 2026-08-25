//! 单生产者单消费者音频环形缓冲区
//!
//! 移植自 `yinhe/crates/yinhe-audio/src/audio_ring.rs`，用于解耦音频渲染线程与 `cpal` 回调线程
//! 解决 `lumino` 旧链路 `BufferedRenderer::recv() 阻塞回调 + Mutex` 导致的 `underrun`/卡顿。
//! 设计要点：`2的幂容量 + 单调 AtomicUsize 读写指针 + Acquire/Release + UnsafeCell<f32>`，
//! `discard_before` 以 `write_position` 为边界保留新音频，避免 `seek/play` 竞态丢开头。

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Inner {
    data: Box<[UnsafeCell<f32>]>,
    capacity: usize,
    read: AtomicUsize,
    write: AtomicUsize,
}

// `Inner` 通过 `AtomicUsize` 保证同步，`UnsafeCell` 仅在持有独占索引区间时访问
unsafe impl Sync for Inner {}

/// 生产者/消费者共享的环对象，`split` 后各持 `Arc<Inner>`
pub struct AudioRing {
    inner: Arc<Inner>,
}

pub struct AudioRingProducer {
    inner: Arc<Inner>,
}

pub struct AudioRingConsumer {
    inner: Arc<Inner>,
}

impl AudioRing {
    pub fn new(capacity: usize) -> Self {
        debug_assert!(capacity.is_power_of_two());
        debug_assert!(capacity > 0);
        let data = (0..capacity)
            .map(|_| UnsafeCell::new(0.0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            inner: Arc::new(Inner {
                data,
                capacity,
                read: AtomicUsize::new(0),
                write: AtomicUsize::new(0),
            }),
        }
    }

    pub fn split(self) -> (AudioRingProducer, AudioRingConsumer) {
        (
            AudioRingProducer {
                inner: Arc::clone(&self.inner),
            },
            AudioRingConsumer { inner: self.inner },
        )
    }
}

impl AudioRingProducer {
    #[inline]
    pub fn capacity(&self) -> usize {
        self.inner.capacity
    }

    /// 当前写入计数（单调递增，不取模），供 `Consumer::discard_before` 作边界
    #[inline]
    pub fn write_position(&self) -> usize {
        self.inner.write.load(Ordering::Relaxed)
    }

    #[inline]
    pub fn len(&self) -> usize {
        let read = self.inner.read.load(Ordering::Acquire);
        let write = self.inner.write.load(Ordering::Relaxed);
        write.wrapping_sub(read)
    }

    #[inline]
    pub fn free_space(&self) -> usize {
        self.capacity().saturating_sub(self.len())
    }

    pub fn push_slice(&mut self, input: &[f32]) -> usize {
        let read = self.inner.read.load(Ordering::Acquire);
        let write = self.inner.write.load(Ordering::Relaxed);
        let available = self.inner.capacity - write.wrapping_sub(read);
        let count = input.len().min(available);
        if count == 0 {
            return 0;
        }
        unsafe {
            copy_into_ring(&self.inner, write, &input[..count]);
        }
        self.inner
            .write
            .store(write.wrapping_add(count), Ordering::Release);
        count
    }
}

impl AudioRingConsumer {
    pub fn pop_into(&mut self, output: &mut [f32]) -> usize {
        let write = self.inner.write.load(Ordering::Acquire);
        let read = self.inner.read.load(Ordering::Relaxed);
        let available = write.wrapping_sub(read);
        let count = output.len().min(available);
        if count == 0 {
            return 0;
        }
        unsafe {
            copy_from_ring(&self.inner, read, &mut output[..count]);
        }
        self.inner
            .read
            .store(read.wrapping_add(count), Ordering::Release);
        count
    }

    /// 丢弃 `write_at_clear` 之前的所有内容，保留之后新推入的音频
    pub fn discard_before(&mut self, write_at_clear: usize) {
        let read = self.inner.read.load(Ordering::Relaxed);
        let stale = write_at_clear.wrapping_sub(read);
        self.inner
            .read
            .store(read.wrapping_add(stale), Ordering::Release);
    }

    #[inline]
    pub fn len(&self) -> usize {
        let write = self.inner.write.load(Ordering::Acquire);
        let read = self.inner.read.load(Ordering::Relaxed);
        write.wrapping_sub(read)
    }
}

unsafe fn copy_into_ring(inner: &Inner, start: usize, input: &[f32]) {
    let mask = inner.capacity - 1;
    for (offset, &sample) in input.iter().enumerate() {
        let index = (start + offset) & mask;
        unsafe {
            *inner.data[index].get() = sample;
        }
    }
}

unsafe fn copy_from_ring(inner: &Inner, start: usize, output: &mut [f32]) {
    let mask = inner.capacity - 1;
    for (offset, sample) in output.iter_mut().enumerate() {
        let index = (start + offset) & mask;
        unsafe {
            *sample = *inner.data[index].get();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_and_pop_preserves_order() {
        let (mut producer, mut consumer) = AudioRing::new(8).split();
        assert_eq!(producer.push_slice(&[1.0, 2.0, 3.0]), 3);
        let mut out = [0.0; 3];
        assert_eq!(consumer.pop_into(&mut out), 3);
        assert_eq!(out, [1.0, 2.0, 3.0]);
    }

    #[test]
    fn wraps_around() {
        let (mut producer, mut consumer) = AudioRing::new(4).split();
        assert_eq!(producer.push_slice(&[1.0, 2.0, 3.0, 4.0]), 4);
        let mut out = [0.0; 3];
        assert_eq!(consumer.pop_into(&mut out), 3);
        assert_eq!(out, [1.0, 2.0, 3.0]);
        assert_eq!(producer.push_slice(&[5.0, 6.0, 7.0]), 3);
        let mut rest = [0.0; 4];
        assert_eq!(consumer.pop_into(&mut rest), 4);
        assert_eq!(rest, [4.0, 5.0, 6.0, 7.0]);
    }

    #[test]
    fn does_not_overwrite_unread_samples() {
        let (mut producer, mut consumer) = AudioRing::new(4).split();
        assert_eq!(producer.push_slice(&[1.0, 2.0, 3.0, 4.0, 5.0]), 4);
        assert_eq!(producer.push_slice(&[6.0]), 0);
        let mut out = [0.0; 4];
        assert_eq!(consumer.pop_into(&mut out), 4);
        assert_eq!(out, [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn discard_before_keeps_audio_pushed_after_marker() {
        let (mut producer, mut consumer) = AudioRing::new(8).split();
        assert_eq!(producer.push_slice(&[1.0, 2.0, 3.0]), 3);
        let marker = producer.write_position();
        assert_eq!(producer.push_slice(&[4.0, 5.0]), 2);
        consumer.discard_before(marker);
        let mut out = [0.0; 4];
        assert_eq!(consumer.pop_into(&mut out), 2);
        assert_eq!(&out[..2], &[4.0, 5.0]);
    }

    #[test]
    fn discard_before_without_new_audio_empties_ring() {
        let (mut producer, mut consumer) = AudioRing::new(8).split();
        assert_eq!(producer.push_slice(&[1.0, 2.0, 3.0]), 3);
        let marker = producer.write_position();
        consumer.discard_before(marker);
        let mut out = [0.0; 3];
        assert_eq!(consumer.pop_into(&mut out), 0);
        assert_eq!(producer.push_slice(&[6.0]), 1);
        assert_eq!(consumer.pop_into(&mut out[..1]), 1);
        assert_eq!(out[0], 6.0);
    }

    #[test]
    fn discard_before_handles_wrap_around() {
        let (mut producer, mut consumer) = AudioRing::new(4).split();
        assert_eq!(producer.push_slice(&[1.0, 2.0, 3.0, 4.0]), 4);
        let mut out = [0.0; 3];
        assert_eq!(consumer.pop_into(&mut out), 3);
        let marker = producer.write_position();
        assert_eq!(producer.push_slice(&[5.0, 6.0, 7.0]), 3);
        consumer.discard_before(marker);
        assert_eq!(consumer.pop_into(&mut out), 3);
        assert_eq!(&out, &[5.0, 6.0, 7.0]);
    }
}
