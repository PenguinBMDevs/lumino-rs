use super::super::MiditrailInstanceGpu;
use crate::pipeline::RenderPipelineBuilder;

pub fn create_render_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    create_instanced_pipeline(
        device,
        bind_group_layout,
        shader,
        "miditrail_render_pipeline",
        true,
    )
}

/// 创建音符渲染管线（不写入深度缓冲，配合 Painter's algorithm 与琴键最后绘制）。
///
/// 参考 Comet MIDITrail：音符先绘制且不写深度，琴键后绘制使用深度测试，
/// 从而保证琴键始终覆盖在音符之上。
pub fn create_note_render_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    create_instanced_pipeline(
        device,
        bind_group_layout,
        shader,
        "miditrail_note_render_pipeline",
        false,
    )
}

/// 创建 Top 视图琴键渲染管线（flat 着色，写深度，琴键最后绘制覆盖音符）。
pub fn create_top_render_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    create_instanced_pipeline(
        device,
        bind_group_layout,
        shader,
        "miditrail_top_render_pipeline",
        true,
    )
}

/// 创建 Top 视图音符渲染管线（flat 着色，不写深度，配合画家算法）。
pub fn create_top_note_render_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
) -> wgpu::RenderPipeline {
    create_instanced_pipeline(
        device,
        bind_group_layout,
        shader,
        "miditrail_top_note_render_pipeline",
        false,
    )
}

fn create_instanced_pipeline(
    device: &wgpu::Device,
    bind_group_layout: &wgpu::BindGroupLayout,
    shader: &wgpu::ShaderModule,
    label: &str,
    depth_write: bool,
) -> wgpu::RenderPipeline {
    // 顶点位置与法线
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
    // 实例数据
    let instance_layout = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<MiditrailInstanceGpu>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &[
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 0,
                shader_location: 2,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32x3,
                offset: 16,
                shader_location: 3,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32,
                offset: 32,
                shader_location: 4,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Uint32,
                offset: 36,
                shader_location: 5,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 40,
                shader_location: 6,
            },
            wgpu::VertexAttribute {
                format: wgpu::VertexFormat::Float32,
                offset: 44,
                shader_location: 7,
            },
        ],
    };

    RenderPipelineBuilder::new(device, label, shader)
        .bind_group(bind_group_layout)
        .vertex_buffer(pos_normal_layout)
        .vertex_buffer(instance_layout)
        .opaque_target(wgpu::TextureFormat::Rgba8Unorm)
        .depth_stencil(Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: depth_write,
            // 回退到 LessEqual（2026-09-05 driven 实验前的原始状态）：
            // UI 实测键盘顶层/前面层异常，先恢复最后已知良好状态再查根因。
            depth_compare: wgpu::CompareFunction::LessEqual,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }))
        .build()
}
