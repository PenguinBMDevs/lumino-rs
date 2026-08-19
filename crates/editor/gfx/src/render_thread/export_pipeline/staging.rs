//! StagingRing — 四重缓冲环（GPU 渲染与 CPU 读回流水线并行）

use std::ptr;
use std::sync::mpsc;

use crate::gpu_resource_tracker::TrackedBuffer;

/// GPU→CPU 读回缓冲区
pub(crate) struct StagingBuffer {
    pub(crate) buffer: TrackedBuffer,
    pub(crate) padded_bytes_per_row: u32,
    pub(crate) unpadded_bytes_per_row: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

struct StagingSlot {
    buffer: Option<StagingBuffer>,
    /// map_async 完成通知
    rx: Option<mpsc::Receiver<Result<(), wgpu::BufferAsyncError>>>,
}

/// 四重缓冲环，实现 GPU 渲染与 CPU 读回的流水线并行
pub(crate) struct StagingRing {
    slots: [StagingSlot; 4],
    next_write: usize,
    next_read: usize,
    inflight: usize,
    device: wgpu::Device,
    /// 帧数据对象池：已写入 ffmpeg 的缓冲区归还后复用，避免每帧大对象堆分配
    frame_pool: Vec<Vec<u8>>,
}

impl StagingRing {
    pub(crate) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let make_slot = || StagingSlot {
            buffer: Some(Self::create_staging_buffer(device, width, height)),
            rx: None,
        };
        Self {
            slots: [make_slot(), make_slot(), make_slot(), make_slot()],
            next_write: 0,
            next_read: 0,
            inflight: 0,
            device: device.clone(),
            frame_pool: Vec::new(),
        }
    }

    fn create_staging_buffer(device: &wgpu::Device, width: u32, height: u32) -> StagingBuffer {
        let bytes_per_pixel = 4u32; // BGRA
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_bytes_per_row = unpadded_bytes_per_row.div_ceil(align) * align;
        let buffer_size = (padded_bytes_per_row * height) as u64;

        let buffer = TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("staging_ring"),
                size: buffer_size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );

        StagingBuffer {
            buffer,
            padded_bytes_per_row,
            unpadded_bytes_per_row,
            width,
            height,
        }
    }

    pub(crate) fn ensure_size(&mut self, width: u32, height: u32) {
        let mut changed = false;
        for slot in &mut self.slots {
            let needs_resize = if let Some(ref buf) = slot.buffer {
                buf.width != width || buf.height != height
            } else {
                false
            };
            if needs_resize {
                // 必须先丢弃 rx 再销毁 buffer：wgpu 在 buffer drop 时会同步触发
                // 挂起的 map_async 回调（Err(MapAborted)），若 rx 仍存活，该错误会
                // 滞留在 channel 中，后续 try_read 的 is_ok() 误判导致
                // get_mapped_range 在已销毁 buffer 上 panic（wgpu_core.rs:2169）。
                slot.rx = None;
                // 旧缓冲由 Option::take 触发 Drop 自动注销内存计数
                slot.buffer.take();
                slot.buffer = Some(Self::create_staging_buffer(&self.device, width, height));
                changed = true;
            }
        }
        if changed {
            self.next_write = 0;
            self.next_read = 0;
            self.inflight = 0;
        }
    }

    pub(crate) fn can_write(&self) -> bool {
        self.inflight < 4
    }

    #[allow(dead_code)]
    fn has_pending(&self) -> bool {
        self.inflight > 0
    }

    pub(crate) fn acquire_write_slot(&mut self) -> usize {
        debug_assert!(self.can_write());
        let idx = self.next_write;
        self.next_write = (self.next_write + 1) % 4;
        idx
    }

    pub(crate) fn write_slot_buffer(&self, slot_idx: usize) -> Option<&StagingBuffer> {
        // 不变式：StagingSlot 创建即含 buffer；ensure_size 重建后立即复填；
        // rebuild_slot 仅当原 buffer 存在（size>0）时调用且重建为 Some → buffer 恒为 Some。
        // 返回 Option 以在极端异常下让调用方安全跳过，而非令渲染线程 panic 致进程崩溃。
        self.slots[slot_idx].buffer.as_ref()
    }

    pub(crate) fn map_after_submit(&mut self, slot_idx: usize) {
        let slot = &mut self.slots[slot_idx];
        // 不变式：同 write_slot_buffer，slot.buffer 恒为 Some
        let Some(buf) = slot.buffer.as_ref() else {
            debug_assert!(false, "staging slot 应有 buffer（创建/重建后恒为 Some）");
            return;
        };
        let slice = buf.buffer.inner().slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        slot.rx = Some(rx);
        self.inflight += 1;
    }

    pub(crate) fn try_read(&mut self) -> Option<Vec<u8>> {
        if self.inflight == 0 {
            return None;
        }
        let _ = self.device.poll(wgpu::PollType::Poll);
        let slot = &self.slots[self.next_read];
        if let Some(ref rx) = slot.rx
            && let Ok(map_result) = rx.try_recv()
        {
            // 必须检查 map 结果：wgpu 在 buffer 销毁/取消 map 时回调
            // Err(BufferAsyncError::Destroyed/MapAborted)，此时 buffer 已不可读，
            // 直接 finish_read 会 get_mapped_range panic。
            if map_result.is_ok() {
                return self.finish_read();
            }
            // map 失败（buffer 已销毁等）：重建该槽位并跳过，避免 panic
            tracing::warn!("staging ring map 失败：{:?}，重建槽位", map_result.err());
            self.rebuild_slot(self.next_read);
            self.next_read = (self.next_read + 1) % 4;
            self.inflight -= 1;
            return None;
        }
        None
    }

    /// 重建槽位的 staging buffer（map 失败/超时后调用，丢弃失效资源）
    fn rebuild_slot(&mut self, slot_idx: usize) {
        let slot = &mut self.slots[slot_idx];
        slot.rx = None;
        let size = slot
            .buffer
            .as_ref()
            .map(|b| (b.width, b.height))
            .unwrap_or((0, 0));
        // 旧缓冲由 Option::take 触发 Drop 自动注销内存计数
        slot.buffer.take();
        if size.0 > 0 && size.1 > 0 {
            slot.buffer = Some(Self::create_staging_buffer(&self.device, size.0, size.1));
        }
    }

    pub(crate) fn wait_read(&mut self) -> Vec<u8> {
        if self.inflight == 0 {
            return Vec::new();
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            {
                let slot = &self.slots[self.next_read];
                if let Some(ref rx) = slot.rx
                    && let Ok(map_result) = rx.try_recv()
                {
                    if map_result.is_ok() {
                        return self.finish_read().unwrap_or_default();
                    }
                    // map 失败：重建槽位并跳过（不进入 finish_read，避免 panic）
                    tracing::warn!(
                        "staging ring wait_read map 失败：{:?}，重建槽位",
                        map_result.err()
                    );
                    self.rebuild_slot(self.next_read);
                    self.next_read = (self.next_read + 1) % 4;
                    self.inflight -= 1;
                    return Vec::new();
                }
            }
            if std::time::Instant::now() >= deadline {
                tracing::warn!("staging ring wait_read 超时 5s");
                // 超时说明该槽位的 map 可能永远不完成（如 GPU 忙/已销毁），
                // 重建槽位并跳过，避免 inflight 计数泄漏导致死锁
                self.rebuild_slot(self.next_read);
                self.next_read = (self.next_read + 1) % 4;
                self.inflight = self.inflight.saturating_sub(1);
                return Vec::new();
            }
            let _ = self.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            });
        }
    }

    fn finish_read(&mut self) -> Option<Vec<u8>> {
        let slot = &mut self.slots[self.next_read];
        slot.rx = None;
        // 不变式：同 write_slot_buffer，slot.buffer 恒为 Some
        let Some(buf) = slot.buffer.as_ref() else {
            debug_assert!(false, "staging slot 应有 buffer（创建/重建后恒为 Some）");
            return None;
        };

        let data = buf.buffer.inner().slice(..).get_mapped_range();
        let total_unpadded = (buf.unpadded_bytes_per_row * buf.height) as usize;

        // 从对象池取出复用，或新建缓冲区；确保容量足够当前尺寸
        let mut result = self
            .frame_pool
            .pop()
            .unwrap_or_else(|| Vec::with_capacity(total_unpadded));
        if result.capacity() < total_unpadded {
            result.reserve(total_unpadded - result.capacity());
        }
        result.clear();

        if buf.padded_bytes_per_row == buf.unpadded_bytes_per_row {
            // 无 padding，直接拷贝
            // Safety: result 容量足够，data 切片有效
            unsafe {
                ptr::copy_nonoverlapping(data.as_ptr(), result.as_mut_ptr(), total_unpadded);
                result.set_len(total_unpadded);
            }
        } else {
            // 逐行去 padding，使用 copy_nonoverlapping 避免 extend_from_slice 的边界检查
            // Safety: 每行数据在 data 范围内，result 容量足够
            let unpadded = buf.unpadded_bytes_per_row as usize;
            let padded = buf.padded_bytes_per_row as usize;
            unsafe {
                for row in 0..buf.height as usize {
                    let src = data.as_ptr().add(row * padded);
                    let dst = result.as_mut_ptr().add(row * unpadded);
                    ptr::copy_nonoverlapping(src, dst, unpadded);
                }
                result.set_len(total_unpadded);
            }
        }
        drop(data);
        buf.buffer.inner().unmap();

        self.next_read = (self.next_read + 1) % 4;
        self.inflight -= 1;
        Some(result)
    }

    /// 将已写入 ffmpeg 的帧缓冲区归还对象池，供下次读回复用
    fn recycle_frame(&mut self, mut frame: Vec<u8>) {
        frame.clear();
        // 限制池大小，避免分辨率切换后占用过量内存
        const MAX_POOL_SIZE: usize = 8;
        if self.frame_pool.len() < MAX_POOL_SIZE {
            self.frame_pool.push(frame);
        }
    }

    /// 非阻塞回收已归还的帧缓冲区
    pub(crate) fn try_recycle(&mut self, rx: &mpsc::Receiver<Vec<u8>>) {
        while let Ok(frame) = rx.try_recv() {
            self.recycle_frame(frame);
        }
    }
}

impl Drop for StagingRing {
    fn drop(&mut self) {
        for slot in &mut self.slots {
            // 先丢弃 rx，避免 wgpu 在 buffer drop 时同步回调 Err 消息滞留
            slot.rx = None;
            // TrackedBuffer Drop 自动注销内存计数
            slot.buffer.take();
        }
    }
}
