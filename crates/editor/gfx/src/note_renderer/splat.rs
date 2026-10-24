//! 像素聚合渲染（splat）：全景亚像素档位把「每音符 1 个点」降为
//! 「每屏幕像素 1 个胜者 + 一次全屏合成」。
//!
//! # 为什么
//!
//! 点图元路径的瓶颈是**每实例固定成本**（RTX 2060 ≈0.7ns/实例，与着色器
//! 复杂度基本无关）：64M 音符 ≈ 45ms。本路径仍处理**每一个音符**（不 LOD、
//! 不裁掉可见音符），但只保留每像素最近的胜者，图元量从 N 降到像素数
//! （1080p ≈ 2M），实测 64M 音符 compute ≈ 12ms。
//!
//! # 语义
//!
//! 与 `onion_note.wgsl::vs_point` + `depth LessEqual` 逐像素一致：
//! - 主音轨优先于洋葱皮（对应区域深度），同区取源索引更小者；
//! - 静音轨不参与；主音轨即使静音也显示；
//! - 颜色 = 实例色，主音轨 = 主轨蓝；
//! - 合成片元写回与点路径同值的 `frag_depth`，后续层（预览）深度关系不变。

use crate::gpu_resource_tracker::TrackedBuffer;
use crate::note_renderer::chunk::ChunkLayout;

mod renderer;

/// 启用 splat 的最小实例数。
///
/// 实测（RTX 2060 / 1080p，全链路 clear+compute+合成）：
/// - 100k 点 0.38ms vs splat 0.45ms（点略优）
/// - 500k 两者 ≈0.9ms（打平）
/// - 1M 点 1.4~2.3ms vs splat 0.86ms（splat 起效）
/// - 8M/16M/64M：splat 1.3/2.1/6.1ms vs 点 10.7/11.9/42.8ms
///
/// 低于该值保留点图元路径（避免小场景白付全屏合成固定成本）。
pub(super) const SPLAT_MIN_INSTANCES: usize = 1_000_000;

const SPLAT_SHADER: &str = include_str!("../shaders/splat.wgsl");
const SPLAT_RESOLVE_SHADER: &str = include_str!("../shaders/splat_resolve.wgsl");

/// 算出的屏幕参数（物理像素尺寸 + 逻辑→物理缩放）。
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct SplatParams {
    width: u32,
    height: u32,
    scale_x: f32,
    scale_y: f32,
    /// compute/resolve 的 chunk 全局基准（source 绑定已切片，shader 内是局部索引）
    chunk_base: u32,
    chunk_count: u32,
    _padding: [u32; 2],
}

/// 每帧 splat 的实例/缓冲区句柄集合（避免 prepare 参数爆炸）。
#[derive(Clone, Copy)]
struct SplatBuffers<'a> {
    source: &'a wgpu::Buffer,
    viewport: &'a wgpu::Buffer,
    view_state: &'a wgpu::Buffer,
    chunk_layout: &'a ChunkLayout,
}

/// 每个 source chunk 的区间（全局实例索引）。
#[derive(Clone, Copy)]
struct SplatChunk {
    base: u32,
    count: u32,
}

/// 每帧 splat 的尺寸/缩放/实例数。
struct SplatFrame {
    physical: (u32, u32),
    scale: (f32, f32),
    instance_count: usize,
}

/// splat 的 GPU 资源与管线（仅横向洋葱皮 + depth 的渲染器持有）。
pub(super) struct SplatState {
    compute_pipeline: wgpu::ComputePipeline,
    resolve_pipeline: wgpu::ComputePipeline,
    composite_pipeline: wgpu::RenderPipeline,
    compute_layout: wgpu::BindGroupLayout,
    resolve_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    depth: TrackedBuffer,
    /// 每像素胜者颜色（0xRRGGBBAA）。composite 不再回读整段 source。
    winner_color: TrackedBuffer,
    /// composite 参数（width/height/scale；chunk 字段为 0）。
    params: TrackedBuffer,
    /// 每 chunk 一个 uniform，避免 queue.write_buffer 在同一 submit 内互相覆盖。
    chunk_params: Vec<TrackedBuffer>,
    chunks: Vec<SplatChunk>,
    compute_binds: Vec<wgpu::BindGroup>,
    resolve_binds: Vec<wgpu::BindGroup>,
    composite_bind: Option<wgpu::BindGroup>,
    width: u32,
    height: u32,
}

impl SplatState {
    pub(super) fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        source: &wgpu::Buffer,
        viewport: &wgpu::Buffer,
        view_state: &wgpu::Buffer,
        chunk_layout: &ChunkLayout,
    ) -> Self {
        let shader = crate::shader::create_shader_module(device, "splat_shader", SPLAT_SHADER);
        let resolve_shader = crate::shader::create_shader_module(
            device,
            "splat_resolve_shader",
            SPLAT_RESOLVE_SHADER,
        );

        let compute_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("splat_compute_layout"),
            entries: &[
                entry(0, wgpu::ShaderStages::COMPUTE, uniform_type()),
                entry(1, wgpu::ShaderStages::COMPUTE, uniform_type()),
                entry(2, wgpu::ShaderStages::COMPUTE, storage_type(true)),
                entry(3, wgpu::ShaderStages::COMPUTE, storage_type(false)),
                entry(4, wgpu::ShaderStages::COMPUTE, uniform_type()),
            ],
        });
        let resolve_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("splat_resolve_layout"),
            entries: &[
                entry(0, wgpu::ShaderStages::COMPUTE, storage_type(true)),
                entry(1, wgpu::ShaderStages::COMPUTE, storage_type(true)),
                entry(2, wgpu::ShaderStages::COMPUTE, storage_type(false)),
                entry(3, wgpu::ShaderStages::COMPUTE, uniform_type()),
            ],
        });
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("splat_composite_layout"),
            entries: &[
                entry(2, wgpu::ShaderStages::FRAGMENT, storage_type(true)),
                entry(3, wgpu::ShaderStages::FRAGMENT, storage_type(false)),
                entry(4, wgpu::ShaderStages::FRAGMENT, uniform_type()),
                entry(5, wgpu::ShaderStages::FRAGMENT, storage_type(true)),
            ],
        });

        let compute_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("splat_compute_pipeline"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("splat_compute_pl"),
                    bind_group_layouts: &[&compute_layout],
                    push_constant_ranges: &[],
                }),
            ),
            module: &shader,
            entry_point: Some("splat"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let resolve_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("splat_resolve_pipeline"),
            layout: Some(
                &device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("splat_resolve_pl"),
                    bind_group_layouts: &[&resolve_layout],
                    push_constant_ranges: &[],
                }),
            ),
            module: &resolve_shader,
            entry_point: Some("resolve"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let composite_pipeline = crate::pipeline::RenderPipelineBuilder::new(
            device,
            "splat_composite_pipeline",
            &shader,
        )
        .vertex_entry("vs_fullscreen")
        .fragment_entry("fs_splat")
        .bind_group(&composite_layout)
        .opaque_target(format)
        .depth_stencil(crate::constants::rendering::depth_stencil_state_for(true))
        .build();

        let depth = TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("splat_depth"),
                size: 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        let winner_color = TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("splat_winner_color"),
                size: 4,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        let params = TrackedBuffer::new_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("splat_params"),
                contents: bytemuck::bytes_of(&SplatParams {
                    width: 1,
                    height: 1,
                    scale_x: 1.0,
                    scale_y: 1.0,
                    chunk_base: 0,
                    chunk_count: 0,
                    _padding: [0; 2],
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );

        let mut state = Self {
            compute_pipeline,
            resolve_pipeline,
            composite_pipeline,
            compute_layout,
            resolve_layout,
            composite_layout,
            depth,
            winner_color,
            params,
            chunk_params: Vec::new(),
            chunks: Vec::new(),
            compute_binds: Vec::new(),
            resolve_binds: Vec::new(),
            composite_bind: None,
            width: 1,
            height: 1,
        };
        state.rebuild_bind_groups(device, source, viewport, view_state, chunk_layout);
        state
    }

    /// 重建绑定（实例缓冲 grow 后句柄会变）。
    ///
    /// source 按 `ChunkLayout` 切成 ≤ `max_storage_buffer_binding_size` 的切片；
    /// 每个 chunk 一个 compute/resolve bind group。composite **不再绑定 source**，
    /// 只读 resolve 产出的 winner_color，避免 310M 时 composite 整段绑定越界。
    pub(super) fn rebuild_bind_groups(
        &mut self,
        device: &wgpu::Device,
        source: &wgpu::Buffer,
        viewport: &wgpu::Buffer,
        view_state: &wgpu::Buffer,
        chunk_layout: &ChunkLayout,
    ) {
        let binding_limit = device.limits().max_storage_buffer_binding_size as u64;
        self.compute_binds.clear();
        self.resolve_binds.clear();
        self.chunks.clear();
        self.chunk_params.clear();
        self.composite_bind = None;

        if self.depth.size() > binding_limit || self.winner_color.size() > binding_limit {
            return;
        }
        let instance_size = std::mem::size_of::<crate::NoteInstance>() as u64;
        let source_instances = (source.size() / instance_size) as usize;
        let chunk_count = chunk_layout
            .chunk_count(source_instances)
            .min(super::chunk::MAX_CHUNKS);
        if chunk_count == 0 {
            return;
        }

        for idx in 0..chunk_count {
            let (start, count) = chunk_layout.chunk_range(source_instances, idx);
            if count == 0 {
                continue;
            }
            let offset = (start as u64) * instance_size;
            let bytes = (count as u64) * instance_size;
            if bytes > binding_limit || offset + bytes > source.size() {
                tracing::error!(
                    "splat: chunk{idx} 越界/超限 offset={offset} bytes={bytes} source={} limit={binding_limit}",
                    source.size()
                );
                self.compute_binds.clear();
                self.resolve_binds.clear();
                self.chunks.clear();
                self.chunk_params.clear();
                return;
            }
            let make_source_binding = || {
                wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: source,
                    offset,
                    size: std::num::NonZeroU64::new(bytes),
                })
            };
            let params = TrackedBuffer::new_init(
                device,
                &wgpu::util::BufferInitDescriptor {
                    label: Some("splat_chunk_params"),
                    contents: bytemuck::bytes_of(&SplatParams {
                        width: 1,
                        height: 1,
                        scale_x: 1.0,
                        scale_y: 1.0,
                        chunk_base: start as u32,
                        chunk_count: count as u32,
                        _padding: [0; 2],
                    }),
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                },
            );
            self.compute_binds
                .push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("splat_compute_bind"),
                    layout: &self.compute_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: viewport.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: view_state.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: make_source_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: self.depth.inner().as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: params.inner().as_entire_binding(),
                        },
                    ],
                }));
            self.resolve_binds
                .push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("splat_resolve_bind"),
                    layout: &self.resolve_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: make_source_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: self.depth.inner().as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: self.winner_color.inner().as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: params.inner().as_entire_binding(),
                        },
                    ],
                }));
            self.chunks.push(SplatChunk {
                base: start as u32,
                count: count as u32,
            });
            self.chunk_params.push(params);
        }

        if !self.compute_binds.is_empty() {
            self.composite_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("splat_composite_bind"),
                layout: &self.composite_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.winner_color.inner().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: self.depth.inner().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self.params.inner().as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 5,
                        resource: self.winner_color.inner().as_entire_binding(),
                    },
                ],
            }));
        }
    }

    /// 确保 depth 缓冲覆盖物理尺寸；变化时重建绑定。
    fn ensure_size(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        buffers: SplatBuffers<'_>,
    ) -> bool {
        let Some(size) = (width as u64)
            .checked_mul(height as u64)
            .and_then(|v| v.checked_mul(4))
        else {
            return false;
        };
        if size == 0 || size > device.limits().max_storage_buffer_binding_size as u64 {
            return false;
        }
        if width == self.width && height == self.height {
            return true;
        }
        self.depth = TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("splat_depth"),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        self.winner_color = TrackedBuffer::new(
            device,
            &wgpu::BufferDescriptor {
                label: Some("splat_winner_color"),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            },
        );
        self.width = width;
        self.height = height;
        self.rebuild_bind_groups(
            device,
            buffers.source,
            buffers.viewport,
            buffers.view_state,
            buffers.chunk_layout,
        );
        true
    }

    /// 准备一帧：写参数 + 清空深度（0 = 无音符，atomicMax 语义）。
    fn prepare(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        frame: SplatFrame,
        buffers: SplatBuffers<'_>,
    ) -> bool {
        puffin::profile_function!();
        if !self.ensure_size(device, frame.physical.0, frame.physical.1, buffers) {
            return false;
        }
        if self.compute_binds.is_empty()
            || self.compute_binds.len() != self.resolve_binds.len()
            || self.composite_bind.is_none()
        {
            return false;
        }
        queue.write_buffer(
            self.params.inner(),
            0,
            bytemuck::bytes_of(&SplatParams {
                width: frame.physical.0,
                height: frame.physical.1,
                scale_x: frame.scale.0,
                scale_y: frame.scale.1,
                chunk_base: 0,
                chunk_count: 0,
                _padding: [0; 2],
            }),
        );

        encoder.clear_buffer(self.depth.inner(), 0, Some(self.depth.size()));

        // 1) 每 chunk 一个 compute pass：遍历该 chunk 的全部实例，原子写全局胜者。
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("splat_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.compute_pipeline);
            for (idx, bind) in self.compute_binds.iter().enumerate() {
                let Some(chunk) = self.chunks.get(idx) else {
                    continue;
                };
                // source buffer 容量通常大于本次上传 count；只扫有效实例。
                let active = (frame.instance_count as u32)
                    .saturating_sub(chunk.base)
                    .min(chunk.count);
                if active == 0 {
                    continue;
                }
                if let Some(params) = self.chunk_params.get(idx) {
                    queue.write_buffer(
                        params.inner(),
                        0,
                        bytemuck::bytes_of(&SplatParams {
                            width: frame.physical.0,
                            height: frame.physical.1,
                            scale_x: frame.scale.0,
                            scale_y: frame.scale.1,
                            chunk_base: chunk.base,
                            chunk_count: active,
                            _padding: [0; 2],
                        }),
                    );
                }
                pass.set_bind_group(0, bind, &[]);
                let groups = active.div_ceil(256).max(1);
                pass.dispatch_workgroups(groups.min(65535), groups.div_ceil(65535), 1);
            }
        }

        // 2) resolve：每像素把胜者颜色从它所属 chunk 解析到 winner_color。
        //    composite 因此不需要再绑定整段 source（>2GB 时也不会越界）。
        let pixels = (frame.physical.0 as u64) * (frame.physical.1 as u64);
        let pixel_groups = u32::try_from(pixels.div_ceil(256))
            .unwrap_or(u32::MAX)
            .max(1);
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("splat_resolve_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.resolve_pipeline);
            for bind in &self.resolve_binds {
                pass.set_bind_group(0, bind, &[]);
                pass.dispatch_workgroups(pixel_groups.min(65535), pixel_groups.div_ceil(65535), 1);
            }
        }
        true
    }

    fn draw<'r>(
        &'r self,
        pass: &mut wgpu::RenderPass<'r>,
        scissor_rect: Option<(u32, u32, u32, u32)>,
    ) -> bool {
        puffin::profile_function!();
        let Some(bind) = &self.composite_bind else {
            return false;
        };
        if let Some((x, y, width, height)) = scissor_rect {
            pass.set_scissor_rect(x, y, width, height);
        }
        pass.set_pipeline(&self.composite_pipeline);
        pass.set_bind_group(0, bind, &[]);
        pass.draw(0..3, 0..1);
        true
    }
}

fn entry(
    binding: u32,
    visibility: wgpu::ShaderStages,
    ty: wgpu::BindingType,
) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty,
        count: None,
    }
}

fn uniform_type() -> wgpu::BindingType {
    wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Uniform,
        has_dynamic_offset: false,
        min_binding_size: None,
    }
}

fn storage_type(read_only: bool) -> wgpu::BindingType {
    wgpu::BindingType::Buffer {
        ty: wgpu::BufferBindingType::Storage { read_only },
        has_dynamic_offset: false,
        min_binding_size: None,
    }
}

#[cfg(test)]
mod chunk_tests {
    use super::*;
    use wgpu::util::DeviceExt;

    /// 2 个 source chunk（每个 64 实例）必须生成 2 组 compute/resolve bind，
    /// 且 chunk base/count 是全局实例索引，证明 splat 不再整段绑定 source。
    #[test]
    fn test_splat_rebuilds_one_bind_group_per_source_chunk() {
        let (device, _queue) = crate::test_gpu::shared_device();
        let notes: Vec<crate::NoteInstance> = (0..128)
            .map(|i| crate::NoteInstance::new(i as f32, 60, 10.0, [1.0; 4], 0))
            .collect();
        let source = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("splat_chunk_source"),
            contents: bytemuck::cast_slice(&notes),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
        let viewport = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("splat_chunk_viewport"),
            contents: &[0u8; 64],
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let view_state = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("splat_chunk_view_state"),
            contents: &[0u8; 8208],
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let layout = ChunkLayout {
            instances_per_chunk: 64,
            slot_align: 256,
        };
        let state = SplatState::new(
            &device,
            wgpu::TextureFormat::Rgba8Unorm,
            &source,
            &viewport,
            &view_state,
            &layout,
        );
        assert_eq!(state.chunks.len(), 2, "2 个 chunk");
        assert_eq!(state.chunks[0].base, 0);
        assert_eq!(state.chunks[0].count, 64);
        assert_eq!(state.chunks[1].base, 64);
        assert_eq!(state.chunks[1].count, 64);
        assert_eq!(state.compute_binds.len(), 2);
        assert_eq!(state.resolve_binds.len(), 2);
        assert!(state.composite_bind.is_some());
    }
}
