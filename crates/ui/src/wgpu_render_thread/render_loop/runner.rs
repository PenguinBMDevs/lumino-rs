use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use iced_wgpu::wgpu;

use super::super::commands::{ControlCommand, RenderCommand};
use super::super::params::RenderParams;
use super::super::stats::RenderStats;
use super::commands::process_commands;
use super::prepare::prepare_renderers;
use super::render_pass::{execute_render_pass, update_stats};
use super::textures::ensure_textures;

/// 运行渲染线程主循环
#[allow(clippy::too_many_arguments)]
pub fn run_render_thread(
    device: wgpu::Device,
    queue: wgpu::Queue,
    texture_format: wgpu::TextureFormat,
    running: Arc<AtomicBool>,
    command_receiver: std::sync::mpsc::Receiver<RenderCommand>,
    latest_texture_clone: Arc<Mutex<Option<Arc<wgpu::Texture>>>>,
    stats_clone: Arc<Mutex<RenderStats>>,
    note_events_rx: std::sync::mpsc::Receiver<lumino_gfx::NoteEvent>,
) {
    tracing::info!("Render thread started");

    // 初始化渲染器
    let mut grid_renderer = lumino_gfx::GridRenderer::new(&device, texture_format);
    let mut note_renderer = lumino_gfx::NoteRenderer::new(&device, &queue, texture_format);
    let mut keyboard_renderer = lumino_gfx::KeyboardRenderer::new(&device, texture_format);
    let mut ruler_renderer = lumino_gfx::RulerRenderer::new(&device, texture_format);

    // 创建洋葱皮位图展示管线 + 绑定组布局（在渲染线程中初始化一次，每帧复用）
    let (onion_display_pipeline, onion_display_layout) =
        create_onion_display_pipeline(&device, texture_format);

    // 渲染循环状态
    let mut frame_count = 0u64;
    let mut fps_update_time = Instant::now();
    let mut current_texture: Option<Arc<wgpu::Texture>> = None;
    let mut depth_texture: Option<wgpu::Texture> = None;
    let mut depth_texture_view: Option<wgpu::TextureView> = None;
    let mut current_size = (0, 0);

    while running.load(Ordering::Relaxed) {
        // 处理所有待处理的命令
        let mut latest_params: Option<RenderParams> = None;
        let mut should_shutdown = false;

        process_commands(&command_receiver, &mut latest_params, &mut should_shutdown);

        if should_shutdown {
            break;
        }

        // 执行渲染（离屏纹理）
        if let Some(ref params) = latest_params {
            puffin::profile_scope!("wgpu_render_thread_frame");
            let frame_start = Instant::now();

            let width = params.viewport_size.0.max(1);
            let height = params.viewport_size.1.max(1);

            // 确保离屏纹理已创建
            ensure_textures(
                &device,
                texture_format,
                width,
                height,
                &mut current_size,
                &mut current_texture,
                &mut depth_texture,
                &mut depth_texture_view,
                &latest_texture_clone,
                params,
            );

            if let (Some(texture), Some(_depth_view)) = (&current_texture, &depth_texture_view) {
                let _view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("offscreen_render_encoder"),
                });

                // 准备渲染器
                prepare_renderers(
                    &mut grid_renderer,
                    &mut note_renderer,
                    &mut keyboard_renderer,
                    &mut ruler_renderer,
                    params,
                    &note_events_rx,
                    &device,
                    &queue,
                );

                // 执行渲染通道
                execute_render_pass(
                    &mut encoder,
                    &current_texture,
                    &depth_texture_view,
                    params,
                    &mut grid_renderer,
                    &mut note_renderer,
                    &mut keyboard_renderer,
                    &mut ruler_renderer,
                    &queue,
                    &device,
                    &onion_display_pipeline,
                    &onion_display_layout,
                );

                // 提交渲染指令
                queue.submit(std::iter::once(encoder.finish()));
            }

            // 更新统计
            let frame_time = frame_start.elapsed();
            update_stats(
                &mut frame_count,
                &mut fps_update_time,
                frame_time,
                params,
                &stats_clone,
            );
        } else {
            // 没有新的渲染参数，短暂休眠避免 CPU 空转
            thread::sleep(Duration::from_micros(100));
        }
    }

    tracing::info!("Render thread stopped");
}

/// 创建洋葱皮位图的展示渲染管线
/// 使用全屏四边形 + 纹理采样，在主渲染通道中绘制位图
/// 返回 (pipeline, bind_group_layout)
fn create_onion_display_pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
) -> (wgpu::RenderPipeline, wgpu::BindGroupLayout) {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("onion_display_shader"),
        source: wgpu::ShaderSource::Wgsl(
            r#"
            struct VertexOutput {
                @builtin(position) position: vec4<f32>,
                @location(0) uv: vec2<f32>,
            };

            @vertex
            fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
                var pos = vec2<f32>(0.0, 0.0);
                var uv = vec2<f32>(0.0, 0.0);
                switch idx {
                    case 0u: { pos = vec2<f32>(-1.0, -1.0); uv = vec2<f32>(0.0, 1.0); }
                    case 1u: { pos = vec2<f32>( 1.0, -1.0); uv = vec2<f32>(1.0, 1.0); }
                    case 2u: { pos = vec2<f32>(-1.0,  1.0); uv = vec2<f32>(0.0, 0.0); }
                    case 3u: { pos = vec2<f32>( 1.0,  1.0); uv = vec2<f32>(1.0, 0.0); }
                    default: { pos = vec2<f32>(0.0, 0.0); uv = vec2<f32>(0.0, 0.0); }
                }
                var output: VertexOutput;
                output.position = vec4<f32>(pos, 0.0, 1.0);
                output.uv = uv;
                return output;
            }

            @group(0) @binding(0)
            var display_texture: texture_2d<f32>;
            @group(0) @binding(1)
            var display_sampler: sampler;

            @fragment
            fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
                return textureSample(display_texture, display_sampler, input.uv);
            }
            "#
            .into(),
        ),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("onion_display_bind_group_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("onion_display_pipeline_layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("onion_display_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: false,
            depth_compare: wgpu::CompareFunction::Always,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    (pipeline, bind_group_layout)
}
