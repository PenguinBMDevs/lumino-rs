//! MidiConsole GPU 渲染器 — 渲染器实现

use wgpu::util::DeviceExt;

use super::{
    ATLAS_CELL_H, ATLAS_CELL_W, ATLAS_COLS, ATLAS_ROWS, BAND_SPEED, CellGpu, GRID_COLS, GRID_ROWS,
    MidiconsoleRenderer, Uniforms, font::build_glyph_atlas,
};

impl MidiconsoleRenderer {
    /// 创建渲染器并烘焙字形图集。
    ///
    /// `width`/`height` 为输出帧尺寸，单字符单元尺寸按
    /// `cell_w = width / GRID_COLS`、`cell_h = height / GRID_ROWS` 推导。
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32) -> Self {
        let cell_w = width as f32 / GRID_COLS as f32;
        let cell_h = height as f32 / GRID_ROWS as f32;
        let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("midiconsole_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/midiconsole.wgsl").into()),
        });

        // —— 烘焙字形图集（CPU 侧 ab_glyph，仅一次）——
        let (atlas_data, atlas_w, atlas_h) =
            build_glyph_atlas().unwrap_or_else(|| (vec![0u8; 0], 1, 1));
        let atlas_tex = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("midiconsole_atlas"),
                size: wgpu::Extent3d {
                    width: atlas_w,
                    height: atlas_h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::LayerMajor,
            &atlas_data,
        );

        let atlas_view = atlas_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("midiconsole_atlas_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..wgpu::SamplerDescriptor::default()
        });

        // —— 单元 / uniform 缓冲 ——
        let cell_count = GRID_COLS * GRID_ROWS;
        let cells_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("midiconsole_cells"),
            size: (cell_count * std::mem::size_of::<CellGpu>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniforms = Uniforms {
            grid_cols: GRID_COLS as u32,
            grid_rows: GRID_ROWS as u32,
            cell_w,
            cell_h,
            atlas_cols: ATLAS_COLS,
            atlas_rows: ATLAS_ROWS,
            atlas_cw: ATLAS_CELL_W as f32,
            atlas_ch: ATLAS_CELL_H as f32,
            frame_w: width as f32,
            frame_h: height as f32,
            band_center: 0.0,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            _pad3: 0.0,
            _pad4: 0.0,
        };
        let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("midiconsole_uniform"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        // —— 绑定组布局 ——
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("midiconsole_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("midiconsole_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: cells_buffer.as_entire_binding(),
                },
            ],
        });

        let pipeline = crate::pipeline::RenderPipelineBuilder::new(
            device,
            "midiconsole_pipeline",
            &shader_module,
        )
        .bind_group(&bind_group_layout)
        .opaque_target(wgpu::TextureFormat::Rgba8Unorm)
        .build();

        let output_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("midiconsole_output"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_texture_view =
            output_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            pipeline,
            bind_group,
            uniform_buffer,
            cells_buffer,
            output_texture,
            output_texture_view,
            cell_w,
            cell_h,
            frame_w: width,
            frame_h: height,
        }
    }

    /// 渲染一帧并以 RGBA 字节返回（未做行对齐 padding）。
    pub fn render_to_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cells: &[CellGpu],
        tick: u32,
    ) -> Vec<u8> {
        let cell_count = GRID_COLS * GRID_ROWS;
        assert_eq!(
            cells.len(),
            cell_count,
            "网格单元数量须等于 GRID_COLS*GRID_ROWS"
        );

        // 更新 uniform（移动高亮带中心随 tick 推进）
        let band_center = (tick as f32 * BAND_SPEED) % self.frame_h as f32;
        let u = Uniforms {
            grid_cols: GRID_COLS as u32,
            grid_rows: GRID_ROWS as u32,
            cell_w: self.cell_w,
            cell_h: self.cell_h,
            atlas_cols: ATLAS_COLS,
            atlas_rows: ATLAS_ROWS,
            atlas_cw: ATLAS_CELL_W as f32,
            atlas_ch: ATLAS_CELL_H as f32,
            frame_w: self.frame_w as f32,
            frame_h: self.frame_h as f32,
            band_center,
            _pad0: 0.0,
            _pad1: 0.0,
            _pad2: 0.0,
            _pad3: 0.0,
            _pad4: 0.0,
        };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&u));
        queue.write_buffer(&self.cells_buffer, 0, bytemuck::cast_slice(cells));

        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("midiconsole_encoder"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("midiconsole_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.output_texture_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        // GPU → CPU 读回（参考 miditrail 预览测试样板）
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let bpp = 4u32;
        let padded = (self.frame_w * bpp).next_multiple_of(align);
        let staging = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("midiconsole_staging"),
            contents: &vec![0u8; (padded * self.frame_h) as usize],
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.output_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(self.frame_h),
                },
            },
            wgpu::Extent3d {
                width: self.frame_w,
                height: self.frame_h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit(std::iter::once(encoder.finish()));

        let slice = staging.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        });
        rx.recv()
            .expect("map_async 回调未收到")
            .expect("map_async 失败");

        let data = slice.get_mapped_range();
        let mut out = vec![0u8; (self.frame_w * self.frame_h * bpp) as usize];
        for y in 0..self.frame_h {
            let rs = (y * padded) as usize;
            for x in 0..self.frame_w {
                let si = rs + (x * bpp) as usize;
                let di = (y * self.frame_w * bpp + x * bpp) as usize;
                out[di..di + 4].copy_from_slice(&data[si..si + 4]);
            }
        }
        drop(data);
        staging.unmap();
        out
    }
}
