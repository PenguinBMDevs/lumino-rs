use crate::pipeline::RenderPipelineBuilder;

/// 创建 GPU-Driven 音符渲染管线（Normal 视图终局路径）。
///
/// 与旧 `note_pipeline` 的差异：
/// - 实例布局 = `NoteInstance` 原字节（16B：start_length + key_color + border），
///   位姿由 `miditrail_note_driven.wgsl` 按实例实时推导；
/// - `depth_write=false`（与 legacy 音符管线一致）：顺序由 compact 承载——FILL
///   按画家序写（见 `bucket_cull.wgsl` `paint_order`），绘制顺序即最终次序。
///   不用真实深度：同键叠音顶面共面，深度只差 ULP，会随帧翻转 winner（真机
///   "疯狂闪烁"实锤；legacy 画家排序注释即为此）。
///
/// 琴键仍走旧管线最后绘制（深度清空后 LessEqual，永远置顶，观感不变）。
pub fn create_note_driven_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    driven_group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    let pos_normal_layout = wgpu::VertexBufferLayout {
        array_stride: 24,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 0,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 12,
                shader_location: 1,
            },
        ],
    };
    // 紧凑实例：vec2(start, length) + u32(key_color) + u32(border)，stride 16。
    let compact_layout = wgpu::VertexBufferLayout {
        array_stride: 16,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x2,
                offset: 0,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32,
                offset: 8,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32,
                offset: 12,
                shader_location: 4,
            },
        ],
    };

    RenderPipelineBuilder::new(device, "miditrail_note_driven_pipeline", shader)
        .bind_group(bind_group_layout)
        .bind_group(driven_group_layout)
        .vertex_buffer(pos_normal_layout)
        .vertex_buffer(compact_layout)
        .opaque_target(wgpu::TextureFormat::Rgba8Unorm)
        .depth_stencil(Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }))
        .build()
}

/// GPU-Driven 参数组布局（group1：位姿参数＋键位表 uniform，顶点着色器只读）。
pub fn create_driven_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("miditrail_driven_group_layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    })
}
