//! 全局桶构建驱动：5 pass LSD 基数排序（分块稳定版）+ key 归约。
//!
//! 每 pass = tile_hist → prefix_tiles → scatter_stable 三个 dispatch，
//! pass 间零回读、零 CPU 参与；尾声一次 1KB 回读算全局 `key_offsets`。
//! 音符字节永不回读、永不二次上传。

use super::support::{make_bind_group, new_storage_buffer, readback_u256, storage_entry};
use super::{
    GlobalBucketError, GlobalBucketIndex, HIST_BYTES, HIST_LEN, KEY_BUCKETS, OFFSETS_LEN,
    exclusive_prefix, sort_passes,
};
use crate::gpu_resource_tracker::TrackedBuffer;

const SHADER: &str = include_str!("../shaders/bucket_sort.wgsl");
const MAX_DISPATCH_X: u32 = 65535;
/// 每 workgroup 处理元素数（与 shader 内 TILE 严格一致）。
const TILE_ITEMS: u32 = 2048;
const OFFSETS_BYTES: u64 = (OFFSETS_LEN * std::mem::size_of::<u32>()) as u64;
const ZEROS_1K: [u8; HIST_LEN * 4] = [0u8; HIST_LEN * 4];

/// 与 shader `SortParams` 严格对齐（16 字节）。
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct SortParamsGpu {
    count: u32,
    shift: u32,
    use_key: u32,
    first_pass: u32,
}

impl GlobalBucketIndex {
    /// 在 GPU 上一次性构建全局桶索引。
    ///
    /// # 参数
    /// - `notes` — 常驻音符缓冲（`NoteInstance` 16B，要求 `STORAGE` 用途；
    ///   与 `cull.wgsl` 绑定要求一致，只读，不移动字节）；
    /// - `note_count` — 有效音符数（`≤ u32::MAX`）。
    ///
    /// # 返回
    /// 常驻 `sort_index` + `key_offsets`（调用方持有复用；数据变更后重建）。
    pub fn build(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        notes: &wgpu::Buffer,
        note_count: usize,
    ) -> Result<Self, GlobalBucketError> {
        let count_u32: u32 =
            u32::try_from(note_count).map_err(|_| GlobalBucketError::CountOverflow(note_count))?;
        // 空集：合法但无需 dispatch，返回全零桶边界。
        let offsets = new_storage_buffer(device, "global_bucket_key_offsets", OFFSETS_BYTES);
        if note_count == 0 {
            queue.write_buffer(offsets.inner(), 0, &[0u8; OFFSETS_LEN * 4]);
            let empty = new_storage_buffer(device, "global_bucket_sort_index", 16);
            return Ok(Self {
                sort_index: empty,
                key_offsets: offsets,
                note_count: 0,
            });
        }

        let resources = BuildResources::new(device, note_count, count_u32, notes)?;
        resources.run_sort_passes(device, queue, count_u32);
        let sort_index = resources.reduce_keys(device, queue, count_u32, &offsets)?;
        Ok(Self {
            sort_index,
            key_offsets: offsets,
            note_count,
        })
    }
}

/// 构建期一次性 GPU 资源集合：排序 pass 与尾声归约共用，避免主函数参数爆炸。
struct BuildResources {
    idx_a: TrackedBuffer,
    idx_b: TrackedBuffer,
    tile_buf: TrackedBuffer,
    key_hist: TrackedBuffer,
    params_buf: TrackedBuffer,
    staging: wgpu::Buffer,
    tile_count: u32,
    tile_hist_pipe: wgpu::ComputePipeline,
    prefix_pipe: wgpu::ComputePipeline,
    scatter_pipe: wgpu::ComputePipeline,
    reduce_pipe: wgpu::ComputePipeline,
    bg_a2b: wgpu::BindGroup,
    bg_b2a: wgpu::BindGroup,
}

impl BuildResources {
    fn new(
        device: &wgpu::Device,
        note_count: usize,
        count_u32: u32,
        notes: &wgpu::Buffer,
    ) -> Result<Self, GlobalBucketError> {
        let index_bytes = (note_count as u64) * (std::mem::size_of::<u32>() as u64);
        let idx_a = new_storage_buffer(device, "global_bucket_index_a", index_bytes);
        let idx_b = new_storage_buffer(device, "global_bucket_index_b", index_bytes);
        let tile_count = (count_u32.div_ceil(TILE_ITEMS)).max(1);
        let tile_buf = TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("global_bucket_tiles"),
                size: (tile_count as u64) * HIST_BYTES,
                // COPY_SRC：调试期回读 tile 行（发布构建保留，单测诊断与离线分析用）。
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            },
        );
        // key 归约输出（1KB，尾声回读）。
        let key_hist = TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("global_bucket_key_hist"),
                size: HIST_BYTES,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            },
        );
        let params_buf = TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("global_bucket_params"),
                size: std::mem::size_of::<SortParamsGpu>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("global_bucket_staging"),
            size: HIST_BYTES,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let pipes = create_pipelines(device)?;
        let (bg_a2b, bg_b2a) = make_bind_groups(
            device,
            &pipes,
            BindGroupArgs {
                notes,
                idx_a: idx_a.inner(),
                idx_b: idx_b.inner(),
                tile_buf: tile_buf.inner(),
                params_buf: params_buf.inner(),
                key_hist: key_hist.inner(),
            },
        );
        Ok(Self {
            idx_a,
            idx_b,
            tile_buf,
            key_hist,
            params_buf,
            staging,
            tile_count,
            tile_hist_pipe: pipes.tile_hist,
            prefix_pipe: pipes.prefix,
            scatter_pipe: pipes.scatter,
            reduce_pipe: pipes.reduce,
            bg_a2b,
            bg_b2a,
        })
    }

    fn run_sort_passes(&self, device: &wgpu::Device, queue: &wgpu::Queue, count_u32: u32) {
        for (p, (shift, use_key)) in sort_passes().iter().enumerate() {
            let first_pass = u32::from(p == 0);
            let bg = if p % 2 == 0 {
                &self.bg_a2b
            } else {
                &self.bg_b2a
            };
            set_params(
                queue,
                self.params_buf.inner(),
                count_u32,
                *shift,
                *use_key,
                first_pass,
            );
            let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("global_bucket_sort_pass"),
            });
            // 每 pass：tile_hist → prefix_tiles → scatter_stable。
            {
                let mut cpass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("global_bucket_tile_hist"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.tile_hist_pipe);
                cpass.set_bind_group(0, bg, &[]);
                cpass.dispatch_workgroups(
                    self.tile_count.min(MAX_DISPATCH_X),
                    self.tile_count.div_ceil(MAX_DISPATCH_X).max(1),
                    1,
                );
            }
            {
                let mut cpass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("global_bucket_prefix_tiles"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.prefix_pipe);
                cpass.set_bind_group(0, bg, &[]);
                cpass.dispatch_workgroups(1, 1, 1);
            }
            {
                let mut cpass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("global_bucket_scatter"),
                    timestamp_writes: None,
                });
                cpass.set_pipeline(&self.scatter_pipe);
                cpass.set_bind_group(0, bg, &[]);
                cpass.dispatch_workgroups(
                    self.tile_count.min(MAX_DISPATCH_X),
                    self.tile_count.div_ceil(MAX_DISPATCH_X).max(1),
                    1,
                );
            }
            // 一次性构建：每 pass 提交后强制同步，确保下一 pass 的 uniform 覆写
            // 不与本 pass 的 dispatch 执行竞速。
            let submitted = queue.submit(Some(enc.finish()));
            let _ = device.poll(wgpu::PollType::Wait {
                submission_index: Some(submitted),
                timeout: Some(std::time::Duration::from_secs(30)),
            });
        }
    }

    fn reduce_keys(
        self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        count_u32: u32,
        offsets: &TrackedBuffer,
    ) -> Result<TrackedBuffer, GlobalBucketError> {
        // 5 个 pass（奇数）→ 结果落在 idx_b。
        let sort_index = self.idx_b;

        // 尾声：对有序结果按 key 做 tile 直方图 + 归约 → 1KB 回读 → 全局桶边界。
        // 5 个 pass 后最后一次绑定方向为 bg_a2b（p=4 偶数，in=A out=B），
        // 此处复用它读取有序结果（binding 1 = idx_a？不——有序结果在 idx_b，
        // 故用 bg_b2a：binding 1 = idx_b）。
        set_params(queue, self.params_buf.inner(), count_u32, 0, true, 0);
        queue.write_buffer(self.key_hist.inner(), 0, &ZEROS_1K);
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("global_bucket_key_reduce"),
        });
        {
            let mut cpass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("global_bucket_key_tile_hist"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.tile_hist_pipe);
            cpass.set_bind_group(0, &self.bg_b2a, &[]);
            cpass.dispatch_workgroups(
                self.tile_count.min(MAX_DISPATCH_X),
                self.tile_count.div_ceil(MAX_DISPATCH_X).max(1),
                1,
            );
        }
        {
            let mut cpass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("global_bucket_reduce_tiles"),
                timestamp_writes: None,
            });
            cpass.set_pipeline(&self.reduce_pipe);
            cpass.set_bind_group(0, &self.bg_b2a, &[]);
            cpass.dispatch_workgroups(1, 1, 1);
        }
        enc.copy_buffer_to_buffer(self.key_hist.inner(), 0, &self.staging, 0, HIST_BYTES);
        queue.submit(Some(enc.finish()));
        let key_counts = readback_u256(device, &self.staging)?;
        // 全局桶边界 = key 计数的互斥前缀和（复用纯函数单测覆盖的逻辑）。
        let pfx = exclusive_prefix(&key_counts);
        let mut key_offsets = [0u32; OFFSETS_LEN];
        key_offsets[..KEY_BUCKETS].copy_from_slice(&pfx);
        key_offsets[KEY_BUCKETS] = pfx[KEY_BUCKETS - 1].saturating_add(key_counts[KEY_BUCKETS - 1]);
        queue.write_buffer(offsets.inner(), 0, bytemuck::cast_slice(&key_offsets));

        // 过程暂存释放（idx_a / tile_buf / key_hist / params / staging 均为一次性资源）。
        drop(self.idx_a);
        drop(self.tile_buf);
        drop(self.key_hist);
        drop(self.params_buf);
        drop(self.staging);

        Ok(sort_index)
    }
}

/// 计算管线集合 + 绑定组布局（绑定组构造需复用布局）。
struct PipelineSet {
    tile_hist: wgpu::ComputePipeline,
    prefix: wgpu::ComputePipeline,
    scatter: wgpu::ComputePipeline,
    reduce: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}

/// 绑定组构造参数（避免函数签名超过参数上限）。
struct BindGroupArgs<'a> {
    notes: &'a wgpu::Buffer,
    idx_a: &'a wgpu::Buffer,
    idx_b: &'a wgpu::Buffer,
    tile_buf: &'a wgpu::Buffer,
    params_buf: &'a wgpu::Buffer,
    key_hist: &'a wgpu::Buffer,
}

fn create_pipelines(device: &wgpu::Device) -> Result<PipelineSet, GlobalBucketError> {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bucket_sort"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("global_bucket_sort_bgl"),
        entries: &[
            storage_entry(0, true),
            storage_entry(1, true),
            storage_entry(2, false),
            storage_entry(3, false),
            wgpu::BindGroupLayoutEntry {
                binding: 4,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            storage_entry(5, false),
        ],
    });
    let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("global_bucket_sort_layout"),
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let entry_pipes = [
        "tile_hist",
        "prefix_tiles",
        "scatter_stable",
        "reduce_tiles",
    ]
    .into_iter()
    .map(|entry| {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("global_bucket_pipe"),
            layout: Some(&pipe_layout),
            module: &module,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        })
    })
    .collect::<Vec<_>>();
    let (tile_hist, prefix, scatter, reduce) = match entry_pipes.as_slice() {
        [a, b, c, d] => (a.clone(), b.clone(), c.clone(), d.clone()),
        _ => {
            return Err(GlobalBucketError::BadLength {
                expected: 4,
                got: entry_pipes.len(),
            });
        }
    };
    Ok(PipelineSet {
        tile_hist,
        prefix,
        scatter,
        reduce,
        layout,
    })
}

fn make_bind_groups(
    device: &wgpu::Device,
    set: &PipelineSet,
    args: BindGroupArgs<'_>,
) -> (wgpu::BindGroup, wgpu::BindGroup) {
    // 两个方向的绑定组（in/out 乒乓；key_hist 在排序 pass 中绑定但不使用）。
    let bg_a2b = make_bind_group(
        device,
        &set.layout,
        args.notes,
        args.idx_a,
        args.idx_b,
        args.tile_buf,
        args.params_buf,
        args.key_hist,
    );
    let bg_b2a = make_bind_group(
        device,
        &set.layout,
        args.notes,
        args.idx_b,
        args.idx_a,
        args.tile_buf,
        args.params_buf,
        args.key_hist,
    );
    (bg_a2b, bg_b2a)
}

fn set_params(
    queue: &wgpu::Queue,
    params_buf: &wgpu::Buffer,
    count: u32,
    shift: u32,
    use_key: bool,
    first_pass: u32,
) {
    let params = SortParamsGpu {
        count,
        shift,
        use_key: u32::from(use_key),
        first_pass,
    };
    queue.write_buffer(params_buf, 0, bytemuck::cast_slice(&[params]));
}
