use lumino_gfx::GpuEpoch;
use wgpu;
use wgpu::BufferBindingType;
use wgpu::util::DeviceExt;

use super::instances::f32_slice_to_bytes;
use super::shaders::{CULL_SHADER, KEYBOARD_SHADER, KEYCOLOR_SHADER, NOTE_SHADER};
use super::{Instance, KeyboardRenderer, NoteUniforms};

/// bind group layout 的缓冲条目种类（纯数据）。
#[derive(Clone, Copy)]
enum BindingKind {
    /// 只读 storage
    ReadOnlyStorage,
    /// 读写 storage
    Storage,
    /// uniform
    Uniform,
}

/// 由 `(binding, visibility, kind)` 表构造 bind group layout。
///
/// 13 个条目的字面量差异只有这三项；`has_dynamic_offset: false`、
/// `min_binding_size: None`、`count: None` 在全部条目上恒等。
fn buffer_layout(
    device: &wgpu::Device,
    label: &str,
    bindings: &[(u32, wgpu::ShaderStages, BindingKind)],
) -> wgpu::BindGroupLayout {
    let entries: Vec<wgpu::BindGroupLayoutEntry> = bindings
        .iter()
        .map(|&(binding, visibility, kind)| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty: wgpu::BindingType::Buffer {
                ty: match kind {
                    BindingKind::ReadOnlyStorage => BufferBindingType::Storage { read_only: true },
                    BindingKind::Storage => BufferBindingType::Storage { read_only: false },
                    BindingKind::Uniform => BufferBindingType::Uniform,
                },
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        })
        .collect();
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some(label),
        entries: &entries,
    })
}

/// 管线布局：单个 bind group、无 push constant。
fn pipeline_layout(
    device: &wgpu::Device,
    label: &str,
    bgl: &wgpu::BindGroupLayout,
) -> wgpu::PipelineLayout {
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts: &[bgl],
        push_constant_ranges: &[],
    })
}

/// 纯 2D 渲染管线：Rgba8Unorm / 无混合 / 无深度 / 无多重采样，入口 `vs` / `fs`。
fn render_pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    buffers: &[wgpu::VertexBufferLayout<'_>],
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some("vs"),
            buffers,
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some("fs"),
            targets: &[Some(wgpu::ColorTargetState {
                format: wgpu::TextureFormat::Rgba8Unorm,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        multiview: None,
        cache: None,
    })
}

/// compute 管线：入口固定 `main`。
fn compute_pipeline(
    device: &wgpu::Device,
    label: &str,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
) -> wgpu::ComputePipeline {
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

impl KeyboardRenderer {
    /// 创建渲染器（键盘 + 音符 + 剔除 三套管线，及复用缓冲）
    ///
    /// `epoch`：创建时所属 GPU 代次（REND-019 PR-B 登记）。
    pub fn new(device: &wgpu::Device, epoch: GpuEpoch) -> Self {
        let (keyboard_shader, note_shader, cull_shader) = create_shader_modules(device);

        let (
            key_bgl,
            note_bind_group_layout,
            cull_bind_group_layout,
            keyboard_pipeline_layout,
            note_pipeline_layout,
            cull_pipeline_layout,
        ) = create_bind_group_layouts(device);

        let (keyboard_pipeline, note_pipeline) = create_keyboard_note_pipelines(
            device,
            &keyboard_pipeline_layout,
            &note_pipeline_layout,
            &keyboard_shader,
            &note_shader,
        );

        let cull_pipeline = create_cull_pipeline(device, &cull_pipeline_layout, &cull_shader);

        let (keycolor_bind_group_layout, keycolor_pipeline) = create_keycolor_pipeline(device);

        let (quad_buffer, uniform_buffer, visible_indices, draw_args) =
            create_shared_buffers(device);

        Self {
            pipeline: keyboard_pipeline,
            quad_buffer,
            note_pipeline,
            note_bind_group_layout,
            uniform_buffer,
            cull_pipeline,
            cull_bind_group_layout,
            visible_indices,
            draw_args,
            key_colors: None,
            keycolor_pipeline,
            keycolor_bind_group_layout,
            key_bgl,
            tex: None,
            tex_view: None,
            last_w: 0,
            last_h: 0,
            last_count: 0,
            instance_buffer: None,
            inst_w: 0,
            inst_h: 0,
            inst_keys: 0,
            epoch,
        }
    }
}

/// `new` 阶段一：三套着色器模块（键盘 / 音符 / 剔除）。
///
/// 创建顺序即后续绑定与管线的顺序，三者互不共享模块。
fn create_shader_modules(
    device: &wgpu::Device,
) -> (wgpu::ShaderModule, wgpu::ShaderModule, wgpu::ShaderModule) {
    let keyboard_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("piano_waterfall_keyboard"),
        source: wgpu::ShaderSource::Wgsl(KEYBOARD_SHADER.into()),
    });
    let note_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("piano_waterfall_note"),
        source: wgpu::ShaderSource::Wgsl(NOTE_SHADER.into()),
    });
    let cull_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("piano_waterfall_cull"),
        source: wgpu::ShaderSource::Wgsl(CULL_SHADER.into()),
    });
    (keyboard_shader, note_shader, cull_shader)
}

/// `new` 阶段二：键盘 / 音符 / 剔除的 bind group layout 及其管线布局。
///
/// 三项条目差异只体现在 `buffer_layout` 的表里；管线布局一并产出，供下一阶段建管线。
fn create_bind_group_layouts(
    device: &wgpu::Device,
) -> (
    wgpu::BindGroupLayout,
    wgpu::BindGroupLayout,
    wgpu::BindGroupLayout,
    wgpu::PipelineLayout,
    wgpu::PipelineLayout,
    wgpu::PipelineLayout,
) {
    // 键盘着色器 bind group layout：活跃键颜色（只读 storage，仅顶点阶段）
    let key_bgl = buffer_layout(
        device,
        "piano_waterfall_key_bgl",
        &[(0, wgpu::ShaderStages::VERTEX, BindingKind::ReadOnlyStorage)],
    );
    let keyboard_pipeline_layout = pipeline_layout(device, "piano_waterfall_keyboard", &key_bgl);

    // 音符：实例 storage + uniform + 可见索引（均仅顶点阶段）
    let note_bind_group_layout = buffer_layout(
        device,
        "piano_waterfall_note_bgl",
        &[
            (0, wgpu::ShaderStages::VERTEX, BindingKind::ReadOnlyStorage),
            (1, wgpu::ShaderStages::VERTEX, BindingKind::Uniform),
            (2, wgpu::ShaderStages::VERTEX, BindingKind::ReadOnlyStorage),
        ],
    );
    let note_pipeline_layout =
        pipeline_layout(device, "piano_waterfall_note", &note_bind_group_layout);

    // 剔除 compute：实例只读 + uniform + 可见索引写入 + 间接绘制参数写入 + uniform
    let cull_bind_group_layout = buffer_layout(
        device,
        "piano_waterfall_cull_bgl",
        &[
            (0, wgpu::ShaderStages::COMPUTE, BindingKind::ReadOnlyStorage),
            (1, wgpu::ShaderStages::COMPUTE, BindingKind::Uniform),
            (2, wgpu::ShaderStages::COMPUTE, BindingKind::Storage),
            (3, wgpu::ShaderStages::COMPUTE, BindingKind::Storage),
            (4, wgpu::ShaderStages::COMPUTE, BindingKind::Uniform),
        ],
    );
    let cull_pipeline_layout =
        pipeline_layout(device, "piano_waterfall_cull", &cull_bind_group_layout);

    (
        key_bgl,
        note_bind_group_layout,
        cull_bind_group_layout,
        keyboard_pipeline_layout,
        note_pipeline_layout,
        cull_pipeline_layout,
    )
}

/// `new` 阶段三：键盘 / 音符两条渲染管线（顶点入口 `vs`、片元入口 `fs`）。
fn create_keyboard_note_pipelines(
    device: &wgpu::Device,
    keyboard_pipeline_layout: &wgpu::PipelineLayout,
    note_pipeline_layout: &wgpu::PipelineLayout,
    keyboard_shader: &wgpu::ShaderModule,
    note_shader: &wgpu::ShaderModule,
) -> (wgpu::RenderPipeline, wgpu::RenderPipeline) {
    let keyboard_pipeline = render_pipeline(
        device,
        "piano_waterfall_keyboard",
        keyboard_pipeline_layout,
        keyboard_shader,
        &[
            wgpu::VertexBufferLayout {
                array_stride: 8,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[wgpu::VertexAttribute {
                    format: wgpu::VertexFormat::Float32x2,
                    offset: 0,
                    shader_location: 0,
                }],
            },
            wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<Instance>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &[
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 0,
                        shader_location: 1,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Float32x4,
                        offset: 16,
                        shader_location: 2,
                    },
                    wgpu::VertexAttribute {
                        format: wgpu::VertexFormat::Uint32,
                        offset: 32,
                        shader_location: 3,
                    },
                ],
            },
        ],
    );

    let note_pipeline = render_pipeline(
        device,
        "piano_waterfall_note",
        note_pipeline_layout,
        note_shader,
        &[],
    );

    (keyboard_pipeline, note_pipeline)
}

/// `new` 阶段三续：剔除 compute 管线（入口固定 `main`）。
fn create_cull_pipeline(
    device: &wgpu::Device,
    cull_pipeline_layout: &wgpu::PipelineLayout,
    cull_shader: &wgpu::ShaderModule,
) -> wgpu::ComputePipeline {
    compute_pipeline(
        device,
        "piano_waterfall_cull",
        cull_pipeline_layout,
        cull_shader,
    )
}

/// `new` 阶段四：活跃键颜色 compute 管线及其绑定布局
/// （notes + uniforms + key_colors + cull_offset）。
fn create_keycolor_pipeline(
    device: &wgpu::Device,
) -> (wgpu::BindGroupLayout, wgpu::ComputePipeline) {
    // 活跃键颜色 compute bind group layout（notes + uniforms + key_colors + cull_offset）
    let keycolor_bind_group_layout = buffer_layout(
        device,
        "piano_waterfall_keycolor_bgl",
        &[
            (0, wgpu::ShaderStages::COMPUTE, BindingKind::ReadOnlyStorage),
            (1, wgpu::ShaderStages::COMPUTE, BindingKind::Uniform),
            (2, wgpu::ShaderStages::COMPUTE, BindingKind::Storage),
            (3, wgpu::ShaderStages::COMPUTE, BindingKind::Uniform),
        ],
    );
    let keycolor_pipeline_layout = pipeline_layout(
        device,
        "piano_waterfall_keycolor",
        &keycolor_bind_group_layout,
    );
    let keycolor_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("piano_waterfall_keycolor"),
        source: wgpu::ShaderSource::Wgsl(KEYCOLOR_SHADER.into()),
    });
    let keycolor_pipeline = compute_pipeline(
        device,
        "piano_waterfall_keycolor",
        &keycolor_pipeline_layout,
        &keycolor_shader,
    );
    (keycolor_bind_group_layout, keycolor_pipeline)
}

/// `new` 阶段五：跨帧复用的静态缓冲（单位四边形 / 音符 uniform / 可见索引 / 间接绘制参数）。
fn create_shared_buffers(
    device: &wgpu::Device,
) -> (wgpu::Buffer, wgpu::Buffer, wgpu::Buffer, wgpu::Buffer) {
    // 单位正方形 [0,1]^2 → 两个三角形（6 顶点）
    let quad: [f32; 12] = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0];
    let quad_bytes = f32_slice_to_bytes(&quad);
    let quad_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("piano_waterfall_keyboard_quad"),
        contents: &quad_bytes,
        usage: wgpu::BufferUsages::VERTEX,
    });

    let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("piano_waterfall_note_uniform"),
        size: std::mem::size_of::<NoteUniforms>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    // 可见索引缓冲（容量随音符数变化，初始 1 避免 0 尺寸）
    let visible_indices = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("piano_waterfall_visible_indices"),
        size: 4,
        usage: wgpu::BufferUsages::STORAGE,
        mapped_at_creation: false,
    });

    // 间接绘制参数缓冲：vertex_count=6, instance_count=0, first=0, first=0
    let draw_args = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("piano_waterfall_draw_args"),
        size: 16,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::INDIRECT
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    (quad_buffer, uniform_buffer, visible_indices, draw_args)
}
