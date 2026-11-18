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
    /// 是否存在未收回的映射请求（`map_after_submit` 置位，`finish_read` / 映射失败清除）。
    ///
    /// 未映射缓冲调用 `unmap` 会产生 wgpu 校验错误（`Buffer is not mapped`，默认
    /// uncaptured handler 只打日志）：导出收尾 Drop 对 4 个槽位逐一 unmap 时，
    /// 只有 1 个在途映射，其余 3 个会各报一条——2026-11-17 用户日志即 3 连发。
    map_pending: bool,
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
    /// 每槽位钢琴卷帘键色（128 键 × RGBA）。与帧数据同槽排队，
    /// 保证 4 槽流水线乱序读回时键色与帧严格对应（否则高亮错帧）。
    key_colors: [Option<[u8; 1024]>; 4],
    /// 最近一次创建/重建槽位的尺寸（缺失槽恢复用；buffer 为 None 时尺寸已丢）。
    size: (u32, u32),
}

impl StagingRing {
    pub(crate) fn new(device: &wgpu::Device, width: u32, height: u32) -> Self {
        let make_slot = || StagingSlot {
            buffer: Some(Self::create_staging_buffer(device, width, height)),
            rx: None,
            map_pending: false,
        };
        Self {
            slots: [make_slot(), make_slot(), make_slot(), make_slot()],
            next_write: 0,
            next_read: 0,
            inflight: 0,
            device: device.clone(),
            frame_pool: Vec::new(),
            key_colors: [None; 4],
            size: (width, height),
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
        self.size = (width, height);
        let mut changed = false;
        for idx in 0..self.slots.len() {
            // 缺失槽（TDR 后留空）同样视为需重建：否则设备恢复后该槽永久是死的，
            // 后续写入会在「未提交槽」上反复跳过，破坏帧序与 inflight 不变式。
            let needs_resize = match &self.slots[idx].buffer {
                Some(buf) => buf.width != width || buf.height != height,
                None => true,
            };
            if needs_resize {
                // 先丢弃 rx、仅在映射在途时 unmap（见 `release_slot_buffer`）。
                self.release_slot_buffer(idx);
                // REND-019 PR-D（计划 F10）：device 丢失（TDR）时禁止在死 device 上
                // 新建 staging 资源（validation 失败/挂起）——留空槽，交由
                // `write_slot_buffer` 的 None 容错跳过本帧，导出由 cancel 标志收场。
                if !crate::device_status::device_lost() {
                    self.slots[idx].buffer =
                        Some(Self::create_staging_buffer(&self.device, width, height));
                } else {
                    tracing::warn!("staging ring ensure_size 短路：device 已丢失（TDR），槽位留空");
                }
                changed = true;
            }
        }
        if changed {
            self.next_write = 0;
            self.next_read = 0;
            self.inflight = 0;
            self.key_colors = [None; 4];
        }
    }

    /// 记录某写槽的钢琴卷帘键色（随 `copy_and_submit` 的帧同槽排队）。
    pub(crate) fn set_slot_key_colors(&mut self, slot_idx: usize, key_colors: Option<[u8; 1024]>) {
        self.key_colors[slot_idx] = key_colors;
    }

    pub(crate) fn can_write(&self) -> bool {
        self.inflight < 4
    }

    /// 尝试获取写槽：队列满或该槽缓冲缺失（且无法重建）时返回 `None`，**不推进游标**。
    ///
    /// 缓冲缺失时先尝试重建，失败（TDR/零尺寸）则返回 `None` 让调用方跳过本帧，
    /// 避免「吞槽位但未提交」导致帧序错乱与 inflight 计数脱钩。
    pub(crate) fn try_acquire_write_slot(&mut self) -> Option<usize> {
        if !self.can_write() {
            return None;
        }
        let idx = self.next_write;
        if self.slots[idx].buffer.is_none() {
            self.rebuild_missing_slot(idx);
        }
        // 重建失败（TDR/零尺寸）：返回 None 且不推进游标
        self.slots[idx].buffer.as_ref()?;
        self.next_write = (self.next_write + 1) % 4;
        Some(idx)
    }

    /// 重建缺失槽位（TDR 后 `ensure_size` 留空的槽；device 已丢失时保持为空）。
    fn rebuild_missing_slot(&mut self, slot_idx: usize) {
        if crate::device_status::device_lost() {
            return;
        }
        let (width, height) = self.size;
        if width == 0 || height == 0 {
            return;
        }
        self.slots[slot_idx].buffer =
            Some(Self::create_staging_buffer(&self.device, width, height));
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
        // map_async 已提交：此刻到 unmap 之间缓冲处于「待映射/已映射」态，
        // 只有这段窗口内调用 unmap 才是合法复位（见 `release_slot_buffer`）。
        slot.map_pending = true;
        self.inflight += 1;
    }

    pub(crate) fn try_read(&mut self) -> Option<(Vec<u8>, Option<[u8; 1024]>)> {
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
            // map 失败（buffer 已销毁等）：重建该槽位并跳过，避免 panic。
            // wgpu 已在失败时清除映射态 → 先清标志，重建路径不得再 unmap。
            tracing::warn!("staging ring map 失败：{:?}，重建槽位", map_result.err());
            self.slots[self.next_read].map_pending = false;
            self.recover_slot_after_map_failure(self.next_read);
            self.next_read = (self.next_read + 1) % 4;
            self.inflight -= 1;
            return None;
        }
        None
    }

    /// 释放槽位缓冲：先丢 `rx`，仅在映射在途时 `unmap`。
    ///
    /// 必须先丢弃 rx 再销毁 buffer：wgpu 在 buffer drop 时会同步触发挂起的
    /// `map_async` 回调（`Err(MapAborted)`），若 rx 仍存活，该错误会滞留在
    /// channel 中，后续 `try_read` 的 `is_ok()` 误判导致 `get_mapped_range`
    /// 在已销毁 buffer 上 panic（wgpu_core.rs:2169）。
    ///
    /// REND-019 PR-D：drop 前复位映射状态（防挂起映射在 Drop 中访问违例）；
    /// 但**未映射缓冲调 `unmap` 是 wgpu 校验错误**（`Buffer is not mapped`），
    /// 以 `map_pending` 严格门控，避免收尾时逐槽刷错误日志。
    fn release_slot_buffer(&mut self, slot_idx: usize) {
        let slot = &mut self.slots[slot_idx];
        slot.rx = None;
        let had_map = std::mem::take(&mut slot.map_pending);
        if let Some(buf) = slot.buffer.take()
            && had_map
        {
            crate::device_status::safe_unmap(buf.buffer.inner());
        }
    }

    /// map 失败/超时后的槽位恢复：**TDR 短路**（REND-019 PR-D，计划 F10）——
    /// device 已丢失时不在死 device 上重建资源（必然失败/挂起），仅丢弃失效
    /// 缓冲，交由导出 cancel 标志收场；正常路径重建该槽位。
    fn recover_slot_after_map_failure(&mut self, slot_idx: usize) {
        if crate::device_status::device_lost() {
            tracing::warn!("staging ring 槽位恢复短路：device 已丢失（TDR），交由 cancel 收场");
            self.release_slot_buffer(slot_idx);
            return;
        }
        self.rebuild_slot(slot_idx);
    }

    /// 重建槽位的 staging buffer（map 失败/超时后调用，丢弃失效资源）
    fn rebuild_slot(&mut self, slot_idx: usize) {
        let size = self.slots[slot_idx]
            .buffer
            .as_ref()
            .map(|b| (b.width, b.height))
            .unwrap_or((0, 0));
        self.release_slot_buffer(slot_idx);
        if size.0 > 0 && size.1 > 0 {
            self.slots[slot_idx].buffer =
                Some(Self::create_staging_buffer(&self.device, size.0, size.1));
        }
    }

    pub(crate) fn wait_read(&mut self) -> (Vec<u8>, Option<[u8; 1024]>) {
        if self.inflight == 0 {
            return (Vec::new(), None);
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            // REND-019 PR-D（R5-b：TDR 快速放弃）——device 已丢失时不等满 5s：
            // 挂起映射永不会完成，槽位立即短路回收，导出由 cancel 收场。
            if crate::device_status::device_lost() {
                tracing::warn!("staging ring wait_read：device 已丢失（TDR），立即放弃等待");
                self.skip_current_slot();
                return (Vec::new(), None);
            }
            {
                let slot = &self.slots[self.next_read];
                if let Some(ref rx) = slot.rx
                    && let Ok(map_result) = rx.try_recv()
                {
                    if map_result.is_ok() {
                        return self.finish_read().unwrap_or_default();
                    }
                    // map 失败：重建槽位并跳过（不进入 finish_read，避免 panic）。
                    // wgpu 已清除映射态 → 先清标志，重建路径不得再 unmap。
                    tracing::warn!(
                        "staging ring wait_read map 失败：{:?}，重建槽位",
                        map_result.err()
                    );
                    self.slots[self.next_read].map_pending = false;
                    self.skip_current_slot();
                    return (Vec::new(), None);
                }
            }
            if std::time::Instant::now() >= deadline {
                tracing::warn!("staging ring wait_read 超时 5s");
                // 超时说明该槽位的 map 可能永远不完成（如 GPU 忙/已销毁），
                // 重建槽位并跳过，避免 inflight 计数泄漏导致死锁
                self.skip_current_slot();
                return (Vec::new(), None);
            }
            // 非阻塞推进回调 + 短睡：`poll(Wait, timeout: None)` 会无限阻塞，
            // 使上方 5s deadline 形同虚设（GPU 忙时导出线程挂死）。
            let _ = self.device.poll(wgpu::PollType::Poll);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    /// 当前读槽读取失败后的统一清理：TDR 短路回收槽位 + 游标推进 + inflight 修正。
    fn skip_current_slot(&mut self) {
        self.recover_slot_after_map_failure(self.next_read);
        self.next_read = (self.next_read + 1) % 4;
        self.inflight = self.inflight.saturating_sub(1);
    }

    fn finish_read(&mut self) -> Option<(Vec<u8>, Option<[u8; 1024]>)> {
        // 映射读回收尾（去 padding memcpy + unmap）：导出 CPU 侧的主要拷贝成本
        crate::profiling::tracy_span!("gfx.export.readback_copy");
        let slot = &mut self.slots[self.next_read];
        slot.rx = None;
        // 不变式：同 write_slot_buffer，slot.buffer 恒为 Some
        let Some(buf) = slot.buffer.as_ref() else {
            debug_assert!(false, "staging slot 应有 buffer（创建/重建后恒为 Some）");
            return None;
        };

        // REND-019（R5-b 实测崩溃根因三）：map 回调 `Ok` 与 `device.destroy()`
        // 之间存在竞态窗口——回调已送达（缓冲视为已映射），但缓冲可能已随 TDR
        // 销毁；此时 `get_mapped_range` 走 wgpu facade panic（wgpu_core.rs:2169），
        // unwind 途中再叠加 unmap 的二次 panic 即 panic-in-panic → 进程 abort
        // （0xC0000409 实测）。防线：①读前复查丢失标志（关闭大多数窗口）；
        // ②catch_unwind 兜底残余竞态——任何 panic 一律按 map 失败处理，绝不抛给上层。
        if crate::device_status::device_lost() {
            tracing::warn!("staging ring finish_read 短路：device 已丢失（TDR）");
            self.skip_current_slot();
            return None;
        }
        let mapped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            buf.buffer.inner().slice(..).get_mapped_range()
        }));
        let Ok(data) = mapped else {
            tracing::warn!("staging ring 读回竞态（缓冲已随 TDR 销毁），按 map 失败处理");
            self.skip_current_slot();
            return None;
        };
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
        crate::device_status::safe_unmap(buf.buffer.inner());
        // 映射已被本次 unmap 收回：清标志，后续重建/Drop 不再重复 unmap
        self.slots[self.next_read].map_pending = false;

        // 键色与帧同槽排队：在推进读游标前取出，保证乱序读回时不串帧
        let key_colors = self.key_colors[self.next_read].take();
        self.next_read = (self.next_read + 1) % 4;
        self.inflight -= 1;
        Some((result, key_colors))
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
        for idx in 0..self.slots.len() {
            // REND-019 PR-D（R5-b 实测崩溃根因二）：drop 前复位映射状态——
            // 挂起/残留映射在死 device 上 Drop 会触发 vkUnmapMemory 访问违例。
            // 只对映射在途的槽位 unmap：未映射槽位调 unmap 会刷 wgpu 校验错误。
            // TrackedBuffer Drop 自动注销内存计数
            self.release_slot_buffer(idx);
        }
    }
}
