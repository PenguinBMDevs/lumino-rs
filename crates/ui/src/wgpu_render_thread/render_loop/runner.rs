use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};

use iced_wgpu::wgpu;
use lumino_gfx::{OnionBgTileRef, SwappableBuffer};

use super::super::commands::RenderCommand;
use super::super::params::RenderParams;
use super::super::stats::RenderStats;
use super::commands::process_commands;
use super::prepare::prepare_renderers;
use super::render_pass::{execute_render_pass, update_stats};
use super::textures::ensure_textures;
use crate::editor::onion_bg_pool::OnionBgTilePool;
use crate::render::onion_bg;

/// 洋葱皮瓦片 uniform 数据（48 bytes，16 字节对齐，匹配 WGSL PushConstants）
#[repr(C)]
struct OnionBgPushConstants {
    position: [f32; 2],
    size: [f32; 2],
    uv_offset: [f32; 2],
    uv_scale: [f32; 2],
    track_index: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

/// 将 OnionBgTileRef 转换为 uniform 数据
impl From<&OnionBgTileRef> for OnionBgPushConstants {
    fn from(tile: &OnionBgTileRef) -> Self {
        Self {
            position: tile.position,
            size: tile.size,
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            track_index: tile.track_index,
            _pad0: 0,
            _pad1: 0,
            _pad2: 0,
        }
    }
}

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
    note_instances_buffer: Arc<SwappableBuffer<lumino_gfx::NoteInstance>>,
    onion_skin_instances_buffer: Arc<SwappableBuffer<lumino_gfx::NoteInstance>>,
    onion_bg_tiles_buffer: Arc<SwappableBuffer<lumino_gfx::OnionBgTileRef>>,
    tile_pool: Option<Arc<Mutex<OnionBgTilePool>>>,
) {
    tracing::info!("Render thread started");

    // 初始化渲染器
    let mut grid_renderer = lumino_gfx::GridRenderer::new(&device, texture_format);
    let mut note_renderer = lumino_gfx::NoteRenderer::new(&device, &queue, texture_format);
    let mut keyboard_renderer = lumino_gfx::KeyboardRenderer::new(&device, texture_format);
    let mut ruler_renderer = lumino_gfx::RulerRenderer::new(&device, texture_format);

    // 渲染循环状态
    let mut frame_count = 0u64;
    let mut fps_update_time = Instant::now();
    let mut current_texture: Option<Arc<wgpu::Texture>> = None;
    let mut depth_texture: Option<wgpu::Texture> = None;
    let mut depth_texture_view: Option<wgpu::TextureView> = None;
    let mut current_size = (0, 0);
    let mut last_note_version: u64 = 0;
    let mut last_onion_version: u64 = 0;
    // 可重用合并缓冲区，避免每帧分配
    let mut merged_instances: Vec<lumino_gfx::NoteInstance> = Vec::new();
    // 瓦片渲染管线（延迟初始化）
    struct TileRenderState {
        pipeline: wgpu::RenderPipeline,
        bind_group_layout: wgpu::BindGroupLayout,
        sampler: wgpu::Sampler,
        /// 每瓦片一个 uniform 缓冲，预写入后复用
        uniform_bufs: Vec<wgpu::Buffer>,
    }
    let mut tile_render: Option<TileRenderState> = None;
    let mut last_bg_version: u64 = 0;
    // 每帧缓存的瓦片引用列表（避免重复锁 pool）
    let mut cached_bg_refs: Vec<OnionBgTileRef> = Vec::new();

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

            // 分别检测主音符和洋葱皮版本号，合并后上传（任一变化都触发上传）
            let note_version = note_instances_buffer.version();
            let onion_version = onion_skin_instances_buffer.version();
            if note_version != last_note_version || onion_version != last_onion_version {
                last_note_version = note_version;
                last_onion_version = onion_version;

                puffin::profile_scope!("upload_note_instances_from_buffer");
                let notes = unsafe { note_instances_buffer.read_buffer() };
                let onion = unsafe { onion_skin_instances_buffer.read_buffer() };

                // 重用到合并缓冲区，避免每帧分配
                merged_instances.clear();
                merged_instances.reserve(notes.len() + onion.len());
                merged_instances.extend_from_slice(notes);
                merged_instances.extend_from_slice(onion);

                note_renderer.upload_instances(&merged_instances, &device, &queue);
            }

            // 检测洋葱皮瓦片 buffer 版本号，读取瓦片引用
            let bg_version = onion_bg_tiles_buffer.version();
            if bg_version != last_bg_version {
                last_bg_version = bg_version;
                let refs = unsafe { onion_bg_tiles_buffer.read_buffer() };
                tracing::info!("[CK2] bg_version={}, refs_count={}", bg_version, refs.len());
                cached_bg_refs.clear();
                cached_bg_refs.extend_from_slice(refs);
            }

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

                // 延迟初始化瓦片渲染管线
                if tile_render.is_none() && tile_pool.is_some() {
                    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                        label: Some("onion_bg_shader"),
                        source: wgpu::ShaderSource::Wgsl(onion_bg::SHADER_SRC.into()),
                    });

                    let bg_layout = device.create_bind_group_layout(
                        &wgpu::BindGroupLayoutDescriptor {
                            label: Some("onion_bg_bind_group_layout"),
                            entries: &[
                                wgpu::BindGroupLayoutEntry {
                                    binding: 0,
                                    visibility: wgpu::ShaderStages::FRAGMENT,
                                    ty: wgpu::BindingType::Texture {
                                        sample_type:
                                            wgpu::TextureSampleType::Float { filterable: true },
                                        view_dimension: wgpu::TextureViewDimension::D2,
                                        multisampled: false,
                                    },
                                    count: None,
                                },
                                wgpu::BindGroupLayoutEntry {
                                    binding: 1,
                                    visibility: wgpu::ShaderStages::FRAGMENT,
                                    ty: wgpu::BindingType::Sampler(
                                        wgpu::SamplerBindingType::Filtering,
                                    ),
                                    count: None,
                                },
                                wgpu::BindGroupLayoutEntry {
                                    binding: 2,
                                    visibility: wgpu::ShaderStages::VERTEX,
                                    ty: wgpu::BindingType::Buffer {
                                        ty: wgpu::BufferBindingType::Uniform,
                                        has_dynamic_offset: false,
                                        min_binding_size: wgpu::BufferSize::new(48),
                                    },
                                    count: None,
                                },
                            ],
                        },
                    );

                    let pipeline_layout = device.create_pipeline_layout(
                        &wgpu::PipelineLayoutDescriptor {
                            label: Some("onion_bg_pipeline_layout"),
                            bind_group_layouts: &[&bg_layout],
                            push_constant_ranges: &[], // no push constants
                        },
                    );

                    let pipeline =
                        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                            label: Some("onion_bg_pipeline"),
                            layout: Some(&pipeline_layout),
                            vertex: wgpu::VertexState {
                                module: &shader,
                                entry_point: Some("vs_main"),
                                buffers: &[],
                                compilation_options:
                                    wgpu::PipelineCompilationOptions::default(),
                            },
                            fragment: Some(wgpu::FragmentState {
                                module: &shader,
                                entry_point: Some("fs_main"),
                                targets: &[Some(wgpu::ColorTargetState {
                                    format: texture_format,
                                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                                    write_mask: wgpu::ColorWrites::ALL,
                                })],
                                compilation_options:
                                    wgpu::PipelineCompilationOptions::default(),
                            }),
                            primitive: wgpu::PrimitiveState {
                                topology: wgpu::PrimitiveTopology::TriangleList,
                                strip_index_format: None,
                                front_face: wgpu::FrontFace::Ccw,
                                cull_mode: Some(wgpu::Face::Back),
                                unclipped_depth: false,
                                polygon_mode: wgpu::PolygonMode::Fill,
                                conservative: false,
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

                    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                        label: Some("onion_bg_sampler"),
                        address_mode_u: wgpu::AddressMode::ClampToEdge,
                        address_mode_v: wgpu::AddressMode::ClampToEdge,
                        address_mode_w: wgpu::AddressMode::ClampToEdge,
                        mag_filter: wgpu::FilterMode::Linear,
                        min_filter: wgpu::FilterMode::Linear,
                        mipmap_filter: wgpu::FilterMode::Linear,
                        lod_min_clamp: 0.0,
                        lod_max_clamp: f32::MAX,
                        compare: None,
                        anisotropy_clamp: 1,
                        border_color: None,
                    });

                    tracing::info!("[CK5] onion_bg pipeline created");
                    tile_render = Some(TileRenderState {
                        pipeline,
                        bind_group_layout: bg_layout,
                        sampler,
                        uniform_bufs: Vec::new(),
                    });
                }

                // 写入洋葱皮瓦片 uniform 数据
                if let Some(ref mut tr) = tile_render {
                    // 按需创建 uniform buffer
                    while tr.uniform_bufs.len() < cached_bg_refs.len() {
                        let buf = device.create_buffer(&wgpu::BufferDescriptor {
                            label: Some("onion_bg_uniform"),
                            size: 256,
                            usage: wgpu::BufferUsages::UNIFORM
                                | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        });
                        tr.uniform_bufs.push(buf);
                    }
                    // 写入当前帧的数据
                    for (i, tile_ref) in cached_bg_refs.iter().enumerate() {
                        let push: OnionBgPushConstants = tile_ref.into();
                        tracing::trace!(
                            "[CK2] uniform[{}]: pos=({},{}) size=({},{}) track={}",
                            i, push.position[0], push.position[1],
                            push.size[0], push.size[1], push.track_index,
                        );
                        let bytes = unsafe {
                            std::slice::from_raw_parts(
                                &push as *const OnionBgPushConstants as *const u8,
                                std::mem::size_of::<OnionBgPushConstants>(),
                            )
                        };
                        queue.write_buffer(&tr.uniform_bufs[i], 0, bytes);
                    }
                }

                tracing::info!("[CK2] before render: bg_refs={}, ubufs={}, tile_render={}",
                    cached_bg_refs.len(),
                    tile_render.as_ref().map(|tr| tr.uniform_bufs.len()).unwrap_or(0),
                    tile_render.is_some(),
                );

                // 执行渲染通道（含瓦片）
                let (tile_pipeline, tile_bg_layout, tile_sampler, tile_uniform_bufs) = match &tile_render {
                    Some(tr) => (
                        Some(&tr.pipeline),
                        Some(&tr.bind_group_layout),
                        Some(&tr.sampler),
                        Some(&tr.uniform_bufs[..]),
                    ),
                    None => (None, None, None, None),
                };
                let (tile_pool_ref, bg_refs) = match &tile_pool {
                    Some(pool) => (Some(pool), cached_bg_refs.as_slice()),
                    None => (None, &[][..]),
                };
                execute_render_pass(
                    &mut encoder,
                    &device,
                    &current_texture,
                    &depth_texture_view,
                    params,
                    &mut grid_renderer,
                    &mut note_renderer,
                    &mut keyboard_renderer,
                    &mut ruler_renderer,
                    &queue,
                    tile_pipeline,
                    tile_bg_layout,
                    tile_pool_ref,
                    tile_sampler,
                    tile_uniform_bufs,
                    bg_refs,
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
