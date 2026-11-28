//! cull 两阶段提取（COUNT 自有提交 + FILL 追加调用方 encoder）。
//!
//! COUNT 与 FILL 同构（同一 shader，同谓词），中间经调用方前缀和衔接；
//! 两阶段划分保证输出 key 连续（legacy 桶内二分前提），无原子竞争。

use super::super::support::readback_bytes_sync;
use super::super::{BucketSource, GlobalBucketError, KEY_BUCKETS, ResidentCull};
use super::{CullActiveParams, CullExtract, CullParamsGpu, CullWindow, missing};

impl ResidentCull {
    /// 提取计数（COUNT：自有 encoder + 提交，回读 1KB 后返回）。
    ///
    /// 回读须在提交后 poll——调用方 encoder 提交时机不定，故 COUNT 自有提交；
    /// FILL 追加到调用方 encoder（与后续渲染同序，一次提交）。
    ///
    /// `active` 为 `Some` 时 COUNT 顺带聚合活跃键/光晕并额外回读 1KB
    ///（miditrail driven 导出：零音符回读替代 `compute_active_and_aura_for_compact`）；
    /// `None` 时 COUNT 循环零额外开销（waterfall 路径）。
    pub fn extract_count(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        resident: &wgpu::Buffer,
        count: usize,
        window: CullWindow,
        active: Option<CullActiveParams>,
    ) -> Result<CullExtract, GlobalBucketError> {
        crate::profiling::tracy_span!("gfx.cull.extract_count");
        let source = BucketSource {
            buffer: resident,
            count,
            // 直绑外部缓冲（钢琴卷帘 onion）用注入的内容世代；上传路径回落到内部 seq。
            epoch: self.external_epoch.unwrap_or(self.seq),
        };
        let key_count = window.key_count.min(KEY_BUCKETS);
        let mut out = CullExtract {
            counts: [0u32; KEY_BUCKETS],
            total: 0,
            bucket_rebuilt: false,
            active: active.map(|_| [0u32; KEY_BUCKETS]),
        };
        if source.count == 0 || key_count == 0 {
            return Ok(out);
        }
        let total_u32 = u32::try_from(source.count)
            .map_err(|_| GlobalBucketError::CountOverflow(source.count))?;
        crate::profiling::tracy_zone!("gfx.cull.ensure_bucket", {
            self.ensure_bucket(device, queue, &source)?;
        });
        out.bucket_rebuilt = self.bucket_rebuilt_flag;
        self.bucket_rebuilt_flag = false;
        self.ensure_cull_resources(device, queue);
        // 游标护栏：tick 倒退（非单调调用）则清零重扫，否则游标位置相对
        // 新 tick 仍有效（死亡单调）。等值 tick 无需处理（非严格单调成立）。
        // 注意 `ensure_bucket` 重建时已清零，此处只处理"桶未变但 tick 回退"。
        if window.tick_start < self.last_tick_start {
            self.zero_cursors(queue);
        }
        self.last_tick_start = window.tick_start;

        let (compute_active, ticks_per_second, fps) = match active {
            Some(p) => (1u32, p.ticks_per_second, p.fps),
            None => (0u32, 0.0, 0.0),
        };
        let params = CullParamsGpu {
            tick_start: window.tick_start,
            tick_end: window.tick_end,
            key_count: key_count as u32,
            phase: 0,
            total_count: total_u32,
            compute_active,
            ticks_per_second,
            fps,
            paint_order: 0,
        };
        let params_buf = self
            .params_buffer
            .as_ref()
            .ok_or(missing("cull 参数缓冲"))?;
        queue.write_buffer(params_buf.inner(), 0, bytemuck::cast_slice(&[params]));
        self.rebuild_bind_group(device, source.buffer)?;
        crate::profiling::tracy_zone!("gfx.cull.dispatch", {
            self.dispatch_count_pass(device, queue, key_count, compute_active)?;
        });
        // 单次同步回读：`[0,1KB)` 计数 +（可选）`[1KB,2KB)` 活跃键。
        // 旧实现每帧两次 map/poll（piano roll 实测 ~5.8ms），合并后减半。
        let bytes = self.read_count_bytes(device, compute_active)?;
        let counts = decode_counts(&bytes, key_count);
        let total = sum_counts(&counts, key_count);
        out.counts = counts;
        out.total = total;
        if compute_active == 1 {
            out.active = Some(decode_active(&bytes));
        }
        Ok(out)
    }

    /// COUNT 阶段的 dispatch + 自有提交（回读须在提交后 poll，故不并入调用方 encoder）。
    fn dispatch_count_pass(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        key_count: usize,
        compute_active: u32,
    ) -> Result<(), GlobalBucketError> {
        let mut count_enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bucket_cull_count"),
        });
        {
            let mut pass = count_enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("bucket_cull_count_pass"),
                timestamp_writes: None,
            });
            let pipeline = self.pipeline.as_ref().ok_or(missing("cull 管线"))?;
            pass.set_pipeline(pipeline);
            let bg = self.bind_group.as_ref().ok_or(missing("cull 绑定组"))?;
            pass.set_bind_group(0, bg, &[]);
            pass.dispatch_workgroups(key_count.div_ceil(64) as u32, 1, 1);
        }
        let counts_inner = self
            .counts_buffer
            .as_ref()
            .ok_or(missing("cull 计数缓冲"))?
            .inner();
        let staging_inner = self
            .counts_staging
            .as_ref()
            .ok_or(missing("cull 回读暂存"))?
            .inner();
        count_enc.copy_buffer_to_buffer(counts_inner, 0, staging_inner, 0, 1024);
        if compute_active == 1 {
            let active_inner = self
                .active_buffer
                .as_ref()
                .ok_or(missing("cull 活跃键缓冲"))?
                .inner();
            // 同一 staging 的后半段：一次 map 同时取回计数与活跃键
            count_enc.copy_buffer_to_buffer(active_inner, 0, staging_inner, 1024, 1024);
        }
        queue.submit(Some(count_enc.finish()));
        Ok(())
    }

    /// 回读 `[0,2KB)`：前半段计数，`compute_active == 1` 时后半段活跃键。
    fn read_count_bytes(
        &self,
        device: &wgpu::Device,
        compute_active: u32,
    ) -> Result<Vec<u8>, GlobalBucketError> {
        let read_len = if compute_active == 1 {
            KEY_BUCKETS * 8
        } else {
            KEY_BUCKETS * 4
        };
        let bytes = crate::profiling::tracy_zone!("gfx.cull.readback", {
            readback_bytes_sync(
                device,
                self.counts_staging
                    .as_ref()
                    .ok_or(missing("cull 回读暂存"))?
                    .inner(),
                read_len,
            )?
        });
        Ok(bytes)
    }

    /// FILL：按调用方前缀和基址写 compact（追加到调用方 encoder，`submit` 后可见）。
    ///
    /// 调用方须保证 `total`/`bases` 与 `extract_count` 返回同源；compact 按 `total`
    /// 内部扩容，句柄变化时重建绑定组，调用方无须关心。
    /// `paint_order=true` 时按画家序写（miditrail；基址须用 `prefix_counts_layered`）。
    /// 参数偏多是 GPU 调度打包（一次 dispatch 免每帧分配），与 `render()` 同策略。
    #[allow(clippy::too_many_arguments)]
    pub fn extract_fill(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        resident: &wgpu::Buffer,
        count: usize,
        window: CullWindow,
        total: usize,
        bases: &[u32; KEY_BUCKETS],
        paint_order: bool,
    ) -> Result<(), GlobalBucketError> {
        let source = BucketSource {
            buffer: resident,
            count,
            // 直绑外部缓冲（钢琴卷帘 onion）用注入的内容世代；上传路径回落到内部 seq。
            epoch: self.external_epoch.unwrap_or(self.seq),
        };
        let key_count = window.key_count.min(KEY_BUCKETS);
        if total == 0 || key_count == 0 {
            return Ok(());
        }
        let total_u32 = u32::try_from(source.count)
            .map_err(|_| GlobalBucketError::CountOverflow(source.count))?;
        self.ensure_compact(device, total);
        self.rebuild_bind_group(device, source.buffer)?;
        let base_buf = self.base_buffer.as_ref().ok_or(missing("cull 基址缓冲"))?;
        queue.write_buffer(base_buf.inner(), 0, bytemuck::cast_slice(bases));
        let params = CullParamsGpu {
            tick_start: window.tick_start,
            tick_end: window.tick_end,
            key_count: key_count as u32,
            phase: 1,
            total_count: total_u32,
            compute_active: 0,
            ticks_per_second: 0.0,
            fps: 0.0,
            paint_order: u32::from(paint_order),
        };
        let params_buf = self
            .params_buffer
            .as_ref()
            .ok_or(missing("cull 参数缓冲"))?;
        queue.write_buffer(params_buf.inner(), 0, bytemuck::cast_slice(&[params]));
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("bucket_cull_fill_pass"),
                timestamp_writes: None,
            });
            let pipeline = self.pipeline.as_ref().ok_or(missing("cull 管线"))?;
            pass.set_pipeline(pipeline);
            let bg = self.bind_group.as_ref().ok_or(missing("cull 绑定组"))?;
            pass.set_bind_group(0, bg, &[]);
            pass.dispatch_workgroups(key_count.div_ceil(64) as u32, 1, 1);
        }
        Ok(())
    }
}

/// 解码 `[0,1KB)` 计数段，并把 `key_count` 之后的尾部清零。
fn decode_counts(bytes: &[u8], key_count: usize) -> [u32; KEY_BUCKETS] {
    let mut counts = [0u32; KEY_BUCKETS];
    for (i, slot) in counts.iter_mut().enumerate() {
        if let Some(c) = bytes.get(i * 4..i * 4 + 4) {
            *slot = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        }
    }
    // 尾部清零：COUNT 内核只写 `[0, key_count)`，旧帧残留不得泄漏。
    for c in counts.iter_mut().skip(key_count) {
        *c = 0;
    }
    counts
}

/// 窗口总数：`counts[..key_count]` 的饱和求和（COUNT 内核只写 `[0, key_count)`）。
fn sum_counts(counts: &[u32; KEY_BUCKETS], key_count: usize) -> usize {
    counts
        .iter()
        .take(key_count)
        .fold(0usize, |acc, &c| acc.saturating_add(c as usize))
}

/// 解码 `[1KB,2KB)` 活跃键段（`[0,128)` 键色，`[128,256)` 光晕系数）。
fn decode_active(bytes: &[u8]) -> [u32; KEY_BUCKETS] {
    let mut arr = [0u32; KEY_BUCKETS];
    for (i, slot) in arr.iter_mut().enumerate() {
        let base = KEY_BUCKETS * 4 + i * 4;
        if let Some(c) = bytes.get(base..base + 4) {
            *slot = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
        }
    }
    arr
}
