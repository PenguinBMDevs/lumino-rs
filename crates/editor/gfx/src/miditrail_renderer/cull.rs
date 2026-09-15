//! Miditrail 导出 cull：首帧全量常驻 + 每帧 GPU 窗口提取 + 回读 legacy 渲染。
//!
//! 背景：24M 级文档下 UI 每帧 collect/sort/pack（~20ms）是导出主瓶颈；全量常驻
//! 曾因“390MB 显存＋390MB 镜像＋每帧两次全扫 100ms”被否决。本路径只保留必要
//! 部分：全量 GPU 常驻一次上传（310MB，与钢琴模式首帧全量同量级，无 CPU 镜像），
//! 每帧 cull 内核按桶分区提取有序窗口（`bucket_cull.wgsl`，与 UI 同谓词），
//! 回读 compact（V×16B）后走未经修改的 legacy `render_from_instances`——
//! 像素与现状逐位一致（集合等价 harness 保证），视觉 veto 不触发。
//!
//! 两次提交：COUNT 自有提交（含 1KB 回读，`ResidentCull` 内部）；FILL + compact
//! 回读自有提交（legacy 渲染需 CPU 切片，读回后新 encoder 渲染）。回读量 V×16B
//!（36 万可见约 6MB），相对省掉的 UI 排序可忽略，打点量化。
//!
//! 两条出口：
//! - `cull_window`：Top 视图/回退用（回读 V×16B → legacy 量化渲染）；
//! - `cull_prepare`：Normal driven 用（零音符回读；COUNT 顺带聚合活跃键/光晕
//!   并回读 1KB，FILL 直写 compact 供 driven 顶点管线直绑）。

use super::MiditrailRenderer;
use super::instances::{
    ActiveKeys, build_driven_params, build_key_instances, emit_aura_instances, update_key_positions,
};
use super::math::build_camera_uniform;
use super::types::MiditrailUniformGpu;
use crate::gpu_resource_tracker::TrackedBuffer;
use crate::readback_bytes_sync;
use crate::{CullWindow, KEY_BUCKETS, NoteInstance, prefix_counts_layered};

/// cull 窗口分段耗时（随 `cull_window`/`cull_prepare` 返回；打点拆分用）。
#[derive(Debug, Default, Clone, Copy)]
pub struct CullTiming {
    /// COUNT 内核 + 提交 + 回读同步（`cull_prepare` 含活跃键 1KB 回读）。
    pub count_us: u64,
    /// `cull_window`：FILL + compact 回读提交 + V×16B 按需映射拷贝；
    /// `cull_prepare`：FILL 调度（无 compact 回读）。
    pub fill_readback_us: u64,
}

/// `cull_prepare` 产物：driven 渲染所需的窗口规模 + 活跃键聚合。
#[derive(Debug, Clone)]
pub struct CullPrepared {
    /// 窗口音符总数（driven `draw_indexed` 实例数）。
    pub total: usize,
    /// 活跃键聚合：`[0,128)` 键色（0 = 未按下），`[128,256)` 光晕系数 bitcast。
    pub active: [u32; KEY_BUCKETS],
    /// 分段耗时（口径见 `CullTiming`）。
    pub timing: CullTiming,
}

impl MiditrailRenderer {
    /// 首帧全量播种：常驻缓冲一次上传 + cull 世代递增（桶下次提取时构建）。    ///
    /// 调用方（导出 handler）在 `params.note_instances` 非空时调用；后续空帧跳过。
    pub fn seed_resident(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        notes: &[NoteInstance],
    ) {
        let need = notes.len().max(1);
        let cap = self.resident_capacity;
        if self.resident_buffer.is_none() || need > cap {
            let new_cap = (need.saturating_mul(6) / 5).max(need + 1024);
            let size = (new_cap * 16) as u64;
            self.resident_buffer = Some(TrackedBuffer::new(
                device,
                &wgpu::BufferDescriptor {
                    label: Some("miditrail_export_resident"),
                    size,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                },
            ));
            self.resident_capacity = new_cap;
        }
        if let Some(ref buf) = self.resident_buffer {
            queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(notes));
        }
        self.resident_count = notes.len();
        self.resident_cull.mark_resident_updated();
    }

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

    /// Normal driven 渲染（零音符回读）：compact 直绑顶点实例缓冲，绘制全窗口。
    ///
    /// CPU 每帧只做：活跃键解码 + 128 键实例/光晕构建 + 相机/参数上传；
    /// 音符换算/排序/gather/上传全部消失（vertex 推导 + 深度测试排序）。
    /// `active` 为 `cull_prepare` 回读的 1KB 聚合（布局见 `CullPrepared`）。
    /// 仅 Normal 视图使用；Top 视图走 `cull_window` + `render_from_instances`。
    #[allow(clippy::too_many_arguments)]
    pub fn render_gpu_driven_from_compact(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        uniform: &MiditrailUniformGpu,
        compact: &wgpu::Buffer,
        note_count: usize,
        active: &[u32; KEY_BUCKETS],
    ) {
        let width = uniform.frame_width;
        let height = uniform.frame_height;
        if width == 0 || height == 0 {
            return;
        }
        self.ensure_output_texture(device, width, height);
        update_key_positions(
            uniform.key_count,
            &mut self.last_key_count,
            &mut self.key_positions,
            &mut self.key_widths,
        );

        let (active_keys, aura_sizes) = decode_active_for_gpu(active, uniform.key_count as usize);
        self.update_key_press_factors(&active_keys, uniform.fps);

        let mut key_instances = std::mem::take(&mut self.scratch_keys);
        key_instances.clear();
        build_key_instances(
            uniform,
            &active_keys,
            &self.key_positions,
            &self.key_widths,
            &self.key_press_factors,
            &mut key_instances,
        );
        self.ensure_instance_buffer(device, key_instances.len());
        if let Some(ref buf) = self.instance_buffer {
            queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(&key_instances));
        }

        let params = build_driven_params(uniform, &self.key_positions, &self.key_widths);
        queue.write_buffer(
            self.driven_params_buffer.inner(),
            0,
            bytemuck::cast_slice(&[params]),
        );
        if self.driven_bind_group.is_none() {
            self.driven_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("miditrail_driven_bind_group"),
                layout: &self.driven_group_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.driven_params_buffer.inner().as_entire_binding(),
                }],
            }));
        }

        let mut aura_instances = std::mem::take(&mut self.scratch_auras);
        aura_instances.clear();
        emit_aura_instances(
            &active_keys,
            &aura_sizes,
            uniform.key_count as usize,
            &self.key_positions,
            &self.key_widths,
            &mut aura_instances,
        );
        self.ensure_aura_instance_buffer(device, aura_instances.len());
        if let Some(ref buf) = self.aura_instance_buffer {
            queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(&aura_instances));
        }
        self.ensure_aura_resources(device, queue);

        let camera = build_camera_uniform(width, height, uniform.view_mode, uniform.z_far_distance);
        queue.write_buffer(
            self.uniform_buffer.inner(),
            0,
            bytemuck::cast_slice(&[camera]),
        );
        if self.bind_group.is_none() {
            self.rebuild_bind_group(device);
        }

        self.execute_driven_pass(
            encoder,
            note_count,
            compact,
            &key_instances,
            &aura_instances,
        );

        self.scratch_keys = key_instances;
        self.scratch_auras = aura_instances;
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

    /// 释放导出全套 GPU/CPU 常驻（完成/取消后由 `FinishVideoExport` 调用）。
    ///
    /// 释放：全量常驻缓冲（`seed_resident` 一次上传，24M 文档约 370MB）、
    /// cull 提取器全套（桶/sort_index/compact，约 300MB）、回读暂存、
    /// 实例缓冲 + Aura 实例缓冲（历史峰值，`next_power_of_two` 只增不减）、
    /// 导出专用 CPU 切片（`cull_cpu`/`scratch_derived`）。
    /// 保留：管线/布局/纹理/采样器（编译产物或小常量，下次导出复用）与
    /// 通用构建暂存（`scratch_notes/keys/auras`，`clear+reserve` 语义，
    /// 下次导出首帧即复用，无需重分配）。
    /// 下次 `seed_resident` 按需重建 + 世代递增 → 桶重建，冷启动与首启一致。
    pub fn release_export_resources(&mut self) {
        self.resident_buffer = None;
        self.resident_capacity = 0;
        self.resident_count = 0;
        self.resident_cull.release();
        self.cull_staging = None;
        self.cull_cpu = Vec::new();
        self.scratch_derived = Vec::new();
        self.instance_buffer = None;
        self.instance_capacity = 0;
        self.aura_instance_buffer = None;
        self.aura_instance_capacity = 0;
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

/// 将 GPU 活跃键聚合解码为 `(ActiveKeys, aura_sizes)`。
///
/// 布局：`[0,128)` 键色（0 = 未按下），`[128,256)` 光晕系数 f32 bitcast。
/// 与 `compute_active_and_aura_for_compact` 的 CPU 语义逐位对齐；keys ≥ key_count
/// 的尾部不采信（防上一帧残留）。
pub(super) fn decode_active_for_gpu(
    active: &[u32; KEY_BUCKETS],
    key_count: usize,
) -> (ActiveKeys, [f32; 128]) {
    let limit = key_count.min(128);
    let mut keys = ActiveKeys {
        pressed: [false; 128],
        colors: [0u32; 128],
    };
    let mut aura_sizes = [0.0f32; 128];
    for k in 0..limit {
        let c = active[k];
        if c != 0 {
            keys.pressed[k] = true;
            keys.colors[k] = c;
        }
        aura_sizes[k] = f32::from_bits(active[128 + k]);
    }
    (keys, aura_sizes)
}
