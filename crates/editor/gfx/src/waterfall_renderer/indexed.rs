//! 全局桶索引渲染路径：绑定常驻缓冲＋置换索引，零上传。
//!
//! 与 legacy `render()` 的区别仅在于音符数据源：
//! legacy 每帧上传窗口子集并派生分桶偏移；本路径绑定 load 后常驻全量缓冲，
//! 经 `GlobalBucketIndex`（一次构建、常驻复用）做 `(key, start)` 有序访问。
//! 着色器除索引间接外与 legacy 逐行一致（见 `waterfall_indexed.wgsl` 头注），
//! 像素等价由 `waterfall_renderer/tests.rs` 保证。

use std::time::Instant;

use super::{WaterfallRenderer, WaterfallUniformGpu};
use crate::{BucketSource, GlobalBucketIndex};

/// 索引渲染缓存：全局桶一次构建，常驻复用。
///
/// 源缓冲句柄（字节数）/数量/世代任一变化即重建；输出纹理变化只重建绑定组。
/// `pub(crate)` 仅因父模块结构体字段类型需要，构造与判定封装在本文件。
pub(crate) struct IndexedCache {
    bucket: GlobalBucketIndex,
    src_bytes: u64,
    src_count: usize,
    src_epoch: u64,
}

impl WaterfallRenderer {
    const INDEXED_SHADER: &'static str = include_str!("../shaders/waterfall_indexed.wgsl");

    /// 确保索引管线与布局已创建（once）。
    fn ensure_indexed_pipeline(&mut self, device: &wgpu::Device) {
        if self.indexed_pipeline.is_some() {
            return;
        }
        let shader = crate::shader::create_shader_module(
            device,
            "waterfall_indexed_shader",
            Self::INDEXED_SHADER,
        );

        // binding 0~4 与 legacy 一致，新增 binding 5 置换索引（只读）。
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("waterfall_indexed_bind_group_layout"),
            entries: &[
                uniform_entry(0),
                storage_ro_entry(1),
                storage_ro_entry(2),
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                storage_ro_entry(4),
                storage_ro_entry(5),
            ],
        });
        let pipeline = crate::pipeline::ComputePipelineBuilder::new(
            device,
            "waterfall_indexed_compute_pipeline",
            &shader,
        )
        .bind_group(&layout)
        .build();
        self.indexed_layout = Some(layout);
        self.indexed_pipeline = Some(pipeline);
    }

    /// 全局桶索引渲染一帧（零上传：常驻缓冲只读绑定）。
    ///
    /// # 参数
    /// - `source` — 常驻缓冲三要素（缓冲/数量/世代）；世代或句柄变化时自动重建全局桶
    ///   （一次性成本，首帧打点），之后每帧仅写 uniform 与键色；
    /// - `active_key_colors` — 128 键活跃色（调用方按现有语义从窗口集派生，过渡期保留）。
    ///
    /// # 返回
    /// `true` 表示已提交索引渲染；`false` 表示前置条件不足（空源/构建失败），
    /// 调用方应回退 legacy 路径。
    pub fn render_indexed(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        params: &WaterfallUniformGpu,
        source: BucketSource<'_>,
        active_key_colors: &[u32; 128],
    ) -> bool {
        let width = params.frame_width;
        let height = params.frame_height;
        if width == 0 || height == 0 || source.count == 0 {
            return false;
        }
        self.ensure_output_texture(device, width, height);
        self.ensure_active_key_colors_buffer(device);
        self.ensure_indexed_pipeline(device);

        // 缓存判定：字节数/数量/世代任一变化 → 重建全局桶（一次性）。
        let src_bytes = source.buffer.size();
        let stale = match self.indexed_cache.as_ref() {
            Some(c) => {
                c.src_bytes != src_bytes
                    || c.src_count != source.count
                    || c.src_epoch != source.epoch
            }
            None => true,
        };
        if stale {
            let t0 = Instant::now();
            match GlobalBucketIndex::build(device, queue, source.buffer, source.count) {
                Ok(bucket) => {
                    tracing::info!(
                        notes = source.count,
                        epoch = source.epoch,
                        build_ms = t0.elapsed().as_secs_f64() * 1000.0,
                        "全局桶构建完成（一次性）",
                    );
                    self.indexed_cache = Some(IndexedCache {
                        bucket,
                        src_bytes,
                        src_count: source.count,
                        src_epoch: source.epoch,
                    });
                    self.indexed_bind_group = None;
                }
                Err(e) => {
                    tracing::error!("全局桶构建失败，回退 legacy 路径: {e}");
                    return false;
                }
            }
        }

        // uniform 与键色上传（复用 legacy 自有缓冲）。
        queue.write_buffer(
            self.uniform_buffer.inner(),
            0,
            bytemuck::cast_slice(&[*params]),
        );
        if let Some(ref buf) = self.active_key_colors_buffer {
            queue.write_buffer(buf.inner(), 0, bytemuck::cast_slice(active_key_colors));
        }

        // 绑定组：源稳定时跨帧复用（legacy 每帧重建，此处是省掉的死工作之一）。
        if self.indexed_bind_group.is_none() {
            let cache = match self.indexed_cache.as_ref() {
                Some(c) => c,
                None => {
                    debug_assert!(false, "indexed_cache 应已构建（stale 分支已处理）");
                    return false;
                }
            };
            let layout = match self.indexed_layout.as_ref() {
                Some(l) => l,
                None => {
                    debug_assert!(false, "indexed_layout 应已创建");
                    return false;
                }
            };
            let out_view = match self.output_texture_view.as_ref() {
                Some(v) => v,
                None => {
                    debug_assert!(false, "output_texture_view 应已创建");
                    return false;
                }
            };
            let colors = match self.active_key_colors_buffer.as_ref() {
                Some(b) => b,
                None => {
                    debug_assert!(false, "active_key_colors_buffer 应已创建");
                    return false;
                }
            };
            self.indexed_bind_group = Some(Self::make_indexed_bind_group(
                device,
                layout,
                self.uniform_buffer.inner(),
                source.buffer,
                colors.inner(),
                out_view,
                cache.bucket.key_offsets_buffer(),
                cache.bucket.sort_index_buffer(),
            ));
        }

        // dispatch（与 legacy 同几何：16×16 workgroup 全屏）。
        let pipeline = match self.indexed_pipeline.as_ref() {
            Some(p) => p,
            None => {
                debug_assert!(false, "indexed_pipeline 应已创建");
                return false;
            }
        };
        let bg = match self.indexed_bind_group.as_ref() {
            Some(b) => b,
            None => {
                debug_assert!(false, "indexed_bind_group 应已创建");
                return false;
            }
        };
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("waterfall_indexed_compute_pass"),
                timestamp_writes: None,
            });
            compute_pass.set_pipeline(pipeline);
            compute_pass.set_bind_group(0, bg, &[]);
            const WORKGROUP_SIZE: u32 = 16;
            compute_pass.dispatch_workgroups(
                width.div_ceil(WORKGROUP_SIZE),
                height.div_ceil(WORKGROUP_SIZE),
                1,
            );
        }
        true
    }

    /// 组装索引绑定组（纯函数：调用方保证句柄有效；源/纹理变化时重建）。
    #[allow(clippy::too_many_arguments)]
    fn make_indexed_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        uniform: &wgpu::Buffer,
        notes: &wgpu::Buffer,
        colors: &wgpu::Buffer,
        out_view: &wgpu::TextureView,
        key_offsets: &wgpu::Buffer,
        sort_index: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("waterfall_indexed_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: notes.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: colors.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(out_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: key_offsets.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: sort_index.as_entire_binding(),
                },
            ],
        })
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn storage_ro_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}
