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

/// 算出的屏幕参数（物理像素尺寸 + 逻辑→物理缩放）。
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct SplatParams {
    width: u32,
    height: u32,
    scale_x: f32,
    scale_y: f32,
}

/// 每帧 splat 的实例/缓冲区句柄集合（避免 prepare 参数爆炸）。
#[derive(Clone, Copy)]
struct SplatBuffers<'a> {
    source: &'a wgpu::Buffer,
    viewport: &'a wgpu::Buffer,
    view_state: &'a wgpu::Buffer,
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
    composite_pipeline: wgpu::RenderPipeline,
    compute_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,
    depth: TrackedBuffer,
    params: TrackedBuffer,
    compute_bind: Option<wgpu::BindGroup>,
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
    ) -> Self {
        let shader = crate::shader::create_shader_module(device, "splat_shader", SPLAT_SHADER);

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
        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("splat_composite_layout"),
            entries: &[
                entry(2, wgpu::ShaderStages::FRAGMENT, storage_type(true)),
                // 同一个 shader global 在 compute 入口声明为 read_write；
                // pipeline layout 必须与 shader 声明一致（FRAGMENT 只读语义不改变）。
                entry(3, wgpu::ShaderStages::FRAGMENT, storage_type(false)),
                entry(4, wgpu::ShaderStages::FRAGMENT, uniform_type()),
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
        let params = TrackedBuffer::new_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("splat_params"),
                contents: bytemuck::bytes_of(&SplatParams {
                    width: 1,
                    height: 1,
                    scale_x: 1.0,
                    scale_y: 1.0,
                }),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );

        let mut state = Self {
            compute_pipeline,
            composite_pipeline,
            compute_layout,
            composite_layout,
            depth,
            params,
            compute_bind: None,
            composite_bind: None,
            width: 1,
            height: 1,
        };
        state.rebuild_bind_groups(device, source, viewport, view_state);
        state
    }

    /// 重建绑定（实例缓冲 grow 后句柄会变；总大小超 binding 上限则禁用 splat）。
    pub(super) fn rebuild_bind_groups(
        &mut self,
        device: &wgpu::Device,
        source: &wgpu::Buffer,
        viewport: &wgpu::Buffer,
        view_state: &wgpu::Buffer,
    ) {
        let limit = device.limits().max_storage_buffer_binding_size as u64;
        if source.size() > limit || self.depth.size() > limit {
            self.compute_bind = None;
            self.composite_bind = None;
            return;
        }
        self.compute_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
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
                    resource: source.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.depth.inner().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.params.inner().as_entire_binding(),
                },
            ],
        }));
        self.composite_bind = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("splat_composite_bind"),
            layout: &self.composite_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: source.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: self.depth.inner().as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: self.params.inner().as_entire_binding(),
                },
            ],
        }));
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
        self.width = width;
        self.height = height;
        self.rebuild_bind_groups(device, buffers.source, buffers.viewport, buffers.view_state);
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
        if self.compute_bind.is_none() || self.composite_bind.is_none() {
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
            }),
        );
        if let Some(bind) = &self.compute_bind {
            encoder.clear_buffer(self.depth.inner(), 0, Some(self.depth.size()));
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("splat_pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.compute_pipeline);
            pass.set_bind_group(0, bind, &[]);
            // 每个音符一个线程（不是每像素）：splat 仍需遍历全部实例
            let groups = u32::try_from(frame.instance_count)
                .unwrap_or(u32::MAX)
                .div_ceil(256)
                .max(1);
            let gx = groups.min(65535);
            let gy = groups.div_ceil(65535);
            pass.dispatch_workgroups(gx, gy, 1);
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
