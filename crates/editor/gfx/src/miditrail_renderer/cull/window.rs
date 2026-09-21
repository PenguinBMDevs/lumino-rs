use super::super::MiditrailRenderer;
use super::*;
use crate::gpu_resource_tracker::TrackedBuffer;
use crate::readback_bytes_sync;
use crate::{CullWindow, NoteInstance, prefix_counts_layered};

impl MiditrailRenderer {
    /// Normal driven 准备：COUNT（顺带活跃键聚合，1KB 回读）→ 前缀和 → FILL。
    ///
    /// 不读回 compact——FILL 直写常驻 `compact_buffer`（调用方用
    /// `cull_compact_buffer()` 拿到句柄交 driven 顶点管线直绑）。CPU 每帧只剩
    /// 2×1KB 回读 + 前缀和 + 128 键实例/光晕构建，零 V 级 CPU 工作。
    /// `active_params` 的 tick 统一取 `window.tick_start`（与 `uniform.tick` 同值）。
    pub fn cull_prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        window: CullWindow,
        active_params: crate::CullActiveParams,
    ) -> Result<CullPrepared, crate::GlobalBucketError> {
        let key_count = window.key_count.min(KEY_BUCKETS);
        let resident = self
            .resident_buffer
            .as_ref()
            .ok_or(crate::GlobalBucketError::CullResource("miditrail 常驻缓冲"))?;
        let resident_inner = resident.inner().clone();
        let resident_count = self.resident_count;
        let t_count = std::time::Instant::now();
        let extract = self.resident_cull.extract_count(
            device,
            queue,
            &resident_inner,
            resident_count,
            window,
            Some(active_params),
        )?;
        let count_us = t_count.elapsed().as_micros() as u64;
        let (_offsets, bases, total) = prefix_counts_layered(&extract.counts, key_count);
        let active = extract.active.unwrap_or([0u32; KEY_BUCKETS]);
        if total == 0 {
            return Ok(CullPrepared {
                total: 0,
                active,
                timing: CullTiming {
                    count_us,
                    fill_readback_us: 0,
                },
            });
        }
        let t_fill = std::time::Instant::now();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("miditrail_cull_fill"),
        });
        self.resident_cull.extract_fill(
            device,
            queue,
            &mut encoder,
            &resident_inner,
            resident_count,
            window,
            total,
            &bases,
            true,
        )?;
        queue.submit(Some(encoder.finish()));
        let fill_us = t_fill.elapsed().as_micros() as u64;
        Ok(CullPrepared {
            total,
            active,
            timing: CullTiming {
                count_us,
                fill_readback_us: fill_us,
            },
        })
    }

    /// 常驻 compact 缓冲句柄（FILL 产物，`NoteInstance` 16B 布局，含 VERTEX 用途）。
    pub fn cull_compact_buffer(&self) -> Option<&wgpu::Buffer> {
        self.resident_cull.compact_buffer()
    }

    /// cull 窗口提取并回读（返回 CPU 切片的所有权 + 分段耗时，调用方渲染后
    /// `restore_window` 归还，跨帧复用零分配；take/restore 为指针移动，无拷贝）。
    ///
    /// 失败（空常驻/构建失败/回读失败）返回 `Err`，调用方回退所带音符。
    pub fn cull_window(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        window: CullWindow,
    ) -> Result<(Vec<NoteInstance>, CullTiming), crate::GlobalBucketError> {
        let key_count = window.key_count.min(KEY_BUCKETS);
        let resident = self
            .resident_buffer
            .as_ref()
            .ok_or(crate::GlobalBucketError::CullResource("miditrail 常驻缓冲"))?;
        let resident_inner = resident.inner().clone();
        let resident_count = self.resident_count;
        let t_count = std::time::Instant::now();
        let extract = self.resident_cull.extract_count(
            device,
            queue,
            &resident_inner,
            resident_count,
            window,
            None,
        )?;
        let count_us = t_count.elapsed().as_micros() as u64;
        let (_offsets, bases, total) = prefix_counts_layered(&extract.counts, key_count);
        if total == 0 {
            return Ok((std::mem::take(&mut self.cull_cpu), CullTiming::default()));
        }
        // FILL + compact 回读：自有 encoder + 提交（legacy 需 CPU 切片，此处同步等待）。
        let t_fill = std::time::Instant::now();
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("miditrail_cull_fill_readback"),
        });
        self.resident_cull.extract_fill(
            device,
            queue,
            &mut encoder,
            &resident_inner,
            resident_count,
            window,
            total,
            &bases,
            true,
        )?;
        let compact = self
            .resident_cull
            .compact_buffer()
            .ok_or(crate::GlobalBucketError::CullResource("miditrail 紧凑缓冲"))?
            .clone();
        let need_bytes = total * 16;
        self.ensure_cull_staging(device, need_bytes);
        let staging = self
            .cull_staging
            .as_ref()
            .ok_or(crate::GlobalBucketError::CullResource("miditrail 回读暂存"))?;
        encoder.copy_buffer_to_buffer(&compact, 0, staging.inner(), 0, need_bytes as u64);
        queue.submit(Some(encoder.finish()));
        let bytes = readback_bytes_sync(
            device,
            self.cull_staging
                .as_ref()
                .ok_or(crate::GlobalBucketError::CullResource("miditrail 回读暂存"))?
                .inner(),
            need_bytes,
        )?;
        let fill_readback_us = t_fill.elapsed().as_micros() as u64;
        // 按需映射已保证长度（`need_bytes` 内对齐截断）；尾部零填充不是音符。
        let bytes = &bytes[..need_bytes.min(bytes.len())];
        let count = bytes.len() / 16;
        let mut out = std::mem::take(&mut self.cull_cpu);
        out.clear();
        out.reserve(count);
        out.extend(
            bytemuck::cast_slice::<u8, NoteInstance>(bytes)
                .iter()
                .copied(),
        );
        let timing = CullTiming {
            count_us,
            fill_readback_us,
        };
        Ok((out, timing))
    }

    /// 归还 cull 窗口缓冲（`cull_window` 返回值的去向；跨帧复用）。
    pub fn restore_window(&mut self, window: Vec<NoteInstance>) {
        self.cull_cpu = window;
    }

    /// 确保回读暂存容量（按需扩容 + 滞后收缩；`MAP_READ | COPY_DST`）。
    ///
    /// 收缩是按需映射的搭档：只增不减会让密集段后的稀疏帧常驻数十 MB
    ///（24M 文档实测密集段后稀疏帧被钉死 ~75ms）。`need < cap/4` 时缩到
    /// 1.2×need；staging 是纯 scratch（每帧全量覆写），重建近乎免费，
    /// 滞后带宽避免在边界反复抖动。
    fn ensure_cull_staging(&mut self, device: &wgpu::Device, need_bytes: usize) {
        let cap = self
            .cull_staging
            .as_ref()
            .map(|b| b.inner().size() as usize)
            .unwrap_or(0);
        if cap >= need_bytes && (need_bytes == 0 || cap < need_bytes.saturating_mul(4)) {
            return;
        }
        let size = cull_staging_size(need_bytes) as u64;
        self.cull_staging = Some(TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("miditrail_cull_staging"),
                size: size.max(16),
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        ));
    }
}

/// 回读暂存容量：1.2×need 保底 +4KB，按 16（NoteInstance 步长）对齐
///（map_async range 对齐要求；调用方 `total * 16` 天然对齐）。
fn cull_staging_size(need_bytes: usize) -> usize {
    (need_bytes.saturating_mul(6) / 5)
        .max(need_bytes + 4096)
        .div_ceil(16)
        * 16
}
