//! 无锁单生产者单消费者 (SPSC) 音频环形缓冲区。
//!
//! Producer 在 renderer 线程写，Consumer 在 cpal 回调读。
//! 容量必须是 2 的幂。使用 `UnsafeCell` + 原子索引实现零拷贝。
//! cpal 回调只调 `pop_into()`，如果缓冲区空就由调用方填静音——**永不阻塞**。

use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Inner {
    data: Box<[UnsafeCell<f32>]>,
    capacity: usize,
    read: AtomicUsize,
    write: AtomicUsize,
}

// SAFETY: SPSC 协议保证 producer 和 consumer 不会同时访问同一 slot。
unsafe impl Sync for Inner {}

/// 单生产者单消费者环形缓冲区。
pub(crate) struct AudioRing {
    inner: Arc<Inner>,
}

pub(crate) struct AudioRingProducer {
    inner: Arc<Inner>,
}

pub(crate) struct AudioRingConsumer {
    inner: Arc<Inner>,
}

impl AudioRing {
    pub(crate) fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two());
        assert!(capacity > 0);
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

    pub(crate) fn split(self) -> (AudioRingProducer, AudioRingConsumer) {
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
    pub(crate) fn capacity(&self) -> usize {
        self.inner.capacity
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        let read = self.inner.read.load(Ordering::Acquire);
        let write = self.inner.write.load(Ordering::Relaxed);
        write.wrapping_sub(read)
    }

    #[inline]
    pub(crate) fn free_space(&self) -> usize {
        self.capacity().saturating_sub(self.len())
    }

    pub(crate) fn push_slice(&mut self, input: &[f32]) -> usize {
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

    pub(crate) fn clear(&mut self) {
        let write = self.inner.write.load(Ordering::Relaxed);
        self.inner.read.store(write, Ordering::Release);
    }
}

impl AudioRingConsumer {
    #[inline]
    pub(crate) fn len(&self) -> usize {
        let write = self.inner.write.load(Ordering::Acquire);
        let read = self.inner.read.load(Ordering::Relaxed);
        write.wrapping_sub(read)
    }

    pub(crate) fn pop_into(&mut self, output: &mut [f32]) -> usize {
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

    pub(crate) fn clear(&mut self) {
        let write = self.inner.write.load(Ordering::Acquire);
        self.inner.read.store(write, Ordering::Release);
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
    fn clear_drops_buffered_samples() {
        let (mut producer, mut consumer) = AudioRing::new(8).split();
        assert_eq!(producer.push_slice(&[1.0, 2.0, 3.0]), 3);
        consumer.clear();

        let mut out = [0.0; 3];
        assert_eq!(consumer.pop_into(&mut out), 0);
        assert_eq!(producer.push_slice(&[4.0, 5.0]), 2);
        assert_eq!(consumer.pop_into(&mut out[..2]), 2);
        assert_eq!(&out[..2], &[4.0, 5.0]);
    }
}
