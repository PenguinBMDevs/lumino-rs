//! egui 网格 → wgpu 27 的最小绘制器。

use std::collections::HashMap;
use std::ops::Range;

use egui::epaint::{ImageData, Primitive};
use egui::{ClippedPrimitive, Rect, TextureId, TexturesDelta};
use wgpu::util::DeviceExt;

use crate::shader;

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

/// 已上传的 egui 纹理。
struct TextureEntry {
    /// 纹理本体（`bind_group` 内部也会持有引用，这里保留便于分块更新）。
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
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
        }
    }

    /// 应用 egui 的纹理增量（字体图集 / 用户纹理）。
    ///
    /// 与 `egui-wgpu` 的契约一致：调用方把 [`TexturesDelta`] 交出来，
    /// 本方法负责全部应用并清空（未应用的增量在析构时会 panic）。
    pub fn update_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        delta: &mut TexturesDelta,
    ) {
        for (id, deltas) in &delta.set {
            for image_delta in deltas {
                let [image_width, image_height] = image_delta.image.size();
                let width = image_width.max(1) as u32;
                let height = image_height.max(1) as u32;
                let ImageData::Color(image) = &image_delta.image;
                let pixels: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect();

                match image_delta.pos {
                    None => {
                        let entry = self.create_texture(device, queue, *id, width, height, &pixels);
                        self.textures.insert(*id, entry);
                    }
                    Some([x, y]) => {
                        if let Some(entry) = self.textures.get(id) {
                            write_texture(
                                queue,
                                &entry.texture,
                                [x as u32, y as u32],
                                width,
                                height,
                                &pixels,
                            );
                        } else {
                            tracing::warn!("egui 纹理 {id:?} 的分块更新缺少纹理，已忽略");
                        }
                    }
                }
            }
        }

        for id in &delta.free {
            self.textures.remove(id);
        }
        delta.clear();
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
            let Some(texture) = self.textures.get(&draw.texture) else {
                tracing::warn!("egui 图元引用了未上传的纹理 {:?}，已跳过", draw.texture);
                continue;
            };

            pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3]);
            pass.set_bind_group(1, &texture.bind_group, &[]);
            pass.draw_indexed(draw.index_range.clone(), 0, 0..1);
            drawn += 1;
        }

        drawn
    }

    /// 创建整张纹理并写入数据。
    fn create_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: TextureId,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> TextureEntry {
        let label = format!("lumino_egui_texture_{id:?}");
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // 与 egui-wgpu 一致：纹理存 gamma 值，着色器负责 sRGB 目标转换。
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&label),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        write_texture(queue, &texture, [0, 0], width, height, pixels);

        TextureEntry {
            texture,
            bind_group,
        }
    }
}

/// 写入一张（子）纹理。
fn write_texture(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    origin: [u32; 2],
    width: u32,
    height: u32,
    pixels: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: origin[0],
                y: origin[1],
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}

/// 逻辑矩形 → 物理像素 scissor（`x, y, width, height`）。
fn scissor_rect(clip: Rect, pixels_per_point: f32, screen: [u32; 2]) -> Option<[u32; 4]> {
    // 与 `egui-wgpu` 的 `ScissorRect::new` 逐行一致：四边先 round 再 clamp。
    let to_pixel = |value: f32| (value * pixels_per_point).round() as u32;
    let min_x = to_pixel(clip.min.x).clamp(0, screen[0]);
    let min_y = to_pixel(clip.min.y).clamp(0, screen[1]);
    let max_x = to_pixel(clip.max.x).clamp(min_x, screen[0]);
    let max_y = to_pixel(clip.max.y).clamp(min_y, screen[1]);
    if max_x <= min_x || max_y <= min_y {
        return None;
    }
    Some([min_x, min_y, max_x - min_x, max_y - min_y])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scissor_clamps_to_target() {
        let clip = Rect::from_min_max(egui::pos2(-10.0, -10.0), egui::pos2(20.0, 20.0));
        assert_eq!(scissor_rect(clip, 1.0, [64, 64]), Some([0, 0, 20, 20]));

        let clip = Rect::from_min_max(egui::pos2(60.0, 60.0), egui::pos2(90.0, 90.0));
        assert_eq!(scissor_rect(clip, 1.0, [64, 64]), Some([60, 60, 4, 4]));

        let clip = Rect::from_min_max(egui::pos2(90.0, 90.0), egui::pos2(120.0, 120.0));
        assert_eq!(scissor_rect(clip, 1.0, [64, 64]), None);
    }

    #[test]
    fn scissor_scales_with_dpi() {
        let clip = Rect::from_min_max(egui::pos2(8.0, 8.0), egui::pos2(24.0, 24.0));
        assert_eq!(scissor_rect(clip, 2.0, [128, 128]), Some([16, 16, 32, 32]));
    }
}
