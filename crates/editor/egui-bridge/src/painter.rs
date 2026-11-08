//! egui 网格 → wgpu 27 的最小绘制器。

use std::collections::HashMap;
use std::ops::Range;

use egui::epaint::Primitive;
use egui::{ClippedPrimitive, Rect, TextureId};
use wgpu::util::DeviceExt;

use crate::shader;

mod scissor;
mod texture;

use scissor::scissor_rect;
use texture::{ExternalTexture, TextureEntry};

/// GPU 顶点：与 `egui::epaint::Vertex` 同布局（点坐标由着色器乘 DPI 缩放）。
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuVertex {
    pos: [f32; 2],
    uv: [f32; 2],
    /// sRGB 打包：`r | g << 8 | b << 16 | a << 24`。
    color: u32,
}

/// 全局 uniform：目标尺寸（物理像素）+ 缩放 + 对齐填充。
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Locals {
    screen_size: [f32; 2],
    pixels_per_point: f32,
    _padding: f32,
}

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Uint32];

/// 一次 `draw_indexed` 所需的索引区间与状态。
struct Draw {
    index_range: Range<u32>,
    clip_rect: Rect,
    texture: TextureId,
}

/// 目标画面信息。
#[derive(Debug, Clone, Copy)]
pub struct Screen {
    /// 目标尺寸（物理像素）。
    pub size_px: [u32; 2],
    /// 每逻辑点的物理像素数（DPI 缩放）。
    pub pixels_per_point: f32,
}

/// egui 绘制器。
pub struct Painter {
    pipeline: wgpu::RenderPipeline,
    locals_buffer: wgpu::Buffer,
    locals_bind_group: wgpu::BindGroup,
    texture_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    textures: HashMap<TextureId, TextureEntry>,
    external: HashMap<TextureId, ExternalTexture>,
}

impl Painter {
    /// 为目标颜色格式创建绘制器。
    ///
    /// `target_format` 决定片元入口：sRGB 目标走 gamma→linear 转换，
    /// 线性目标直接输出 gamma 值（与 `egui-wgpu` 一致）。
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lumino_egui_bridge_shader"),
            source: wgpu::ShaderSource::Wgsl(shader::EGUI_WGSL.into()),
        });

        let locals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lumino_egui_bridge_locals_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<Locals>() as u64),
                },
                count: None,
            }],
        });

        let locals_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("lumino_egui_bridge_locals"),
            size: std::mem::size_of::<Locals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let locals_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("lumino_egui_bridge_locals_bind_group"),
            layout: &locals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: locals_buffer.as_entire_binding(),
            }],
        });

        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("lumino_egui_bridge_texture_layout"),
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("lumino_egui_bridge_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("lumino_egui_bridge_pipeline_layout"),
            bind_group_layouts: &[&locals_layout, &texture_layout],
            push_constant_ranges: &[],
        });

        let fragment_entry = if target_format.is_srgb() {
            shader::FRAGMENT_SRGB_ENTRY
        } else {
            shader::FRAGMENT_GAMMA_ENTRY
        };

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lumino_egui_bridge_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some(shader::VERTEX_ENTRY),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &VERTEX_ATTRIBUTES,
                }],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(fragment_entry),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: Some(wgpu::BlendState {
                        // egui 顶点色与纹理均为预乘 alpha。
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            locals_buffer,
            locals_bind_group,
            texture_layout,
            sampler,
            textures: HashMap::new(),
            external: HashMap::new(),
        }
    }

    /// 绘制一帧图元，返回实际提交的 draw 数。
    ///
    /// 采用 `LoadOp::Load`，即叠加在已有画面上（UI 在 3D 场景之后绘制）。
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        screen: Screen,
        primitives: &[ClippedPrimitive],
    ) -> usize {
        if primitives.is_empty() {
            return 0;
        }

        let width = screen.size_px[0].max(1);
        let height = screen.size_px[1].max(1);
        let locals = Locals {
            screen_size: [width as f32, height as f32],
            pixels_per_point: screen.pixels_per_point,
            _padding: 0.0,
        };
        queue.write_buffer(&self.locals_buffer, 0, bytemuck::bytes_of(&locals));

        let mut vertices: Vec<GpuVertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut draws: Vec<Draw> = Vec::new();

        for clipped in primitives {
            let Primitive::Mesh(mesh) = &clipped.primitive else {
                continue;
            };
            if mesh.indices.is_empty() {
                continue;
            }

            let base = vertices.len() as u32;
            vertices.extend(mesh.vertices.iter().map(|vertex| GpuVertex {
                pos: [vertex.pos.x, vertex.pos.y],
                uv: [vertex.uv.x, vertex.uv.y],
                color: u32::from_le_bytes([
                    vertex.color.r(),
                    vertex.color.g(),
                    vertex.color.b(),
                    vertex.color.a(),
                ]),
            }));

            let start = indices.len() as u32;
            indices.extend(mesh.indices.iter().map(|index| index.saturating_add(base)));
            draws.push(Draw {
                index_range: start..indices.len() as u32,
                clip_rect: clipped.clip_rect,
                texture: mesh.texture_id,
            });
        }

        if draws.is_empty() {
            return 0;
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("lumino_egui_bridge_vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("lumino_egui_bridge_indices"),
            contents: bytemuck::cast_slice(&indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("lumino_egui_bridge_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.locals_bind_group, &[]);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.set_index_buffer(index_buffer.slice(..), wgpu::IndexFormat::Uint32);

        let mut drawn = 0;
        for draw in &draws {
            let Some(scissor) =
                scissor_rect(draw.clip_rect, screen.pixels_per_point, [width, height])
            else {
                continue;
            };
            let Some(texture) = self
                .textures
                .get(&draw.texture)
                .map(|entry| &entry.bind_group)
                .or_else(|| {
                    self.external
                        .get(&draw.texture)
                        .map(|entry| &entry.bind_group)
                })
            else {
                tracing::warn!("egui 图元引用了未上传的纹理 {:?}，已跳过", draw.texture);
                continue;
            };

            pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
            pass.set_bind_group(1, texture, &[]);
            pass.draw_indexed(draw.index_range.clone(), 0, 0..1);
            drawn += 1;
        }

        drawn
    }
}
