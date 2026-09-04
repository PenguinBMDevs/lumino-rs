//! MidiConsole 复古终端 GPU 渲染器
//!
//! 将 MidiConsole 风格的字符网格（由 CPU 廉价构建）在 GPU 上栅格化：
//! - 字形图集（r8 覆盖率）在初始化时由 ab_glyph 烘焙一次并上传；
//! - 每帧仅上传网格单元（148×40 个 `CellGpu`，约 71KB）到只读存储缓冲；
//! - 全屏片元着色器完成字形采样、前/背景混合与 CRT 扫描线 + 移动高亮带；
//! - 输出 `Rgba8Unorm` 离屏纹理，可由导出管线读回 CPU 编码视频帧。
//!
//! 相比 CPU 路径（每帧对每字形做 ab_glyph 描边 + 全帧 CRT 逐像素），
//! GPU 路径将两项昂贵工作全部搬上 GPU，导出速度显著更快。

use std::path::PathBuf;

use ab_glyph::{Font, FontArc, FontVec, Point, PxScale, ScaleFont};
use wgpu::util::DeviceExt;

/// 网格列数（与 CPU 风格一致）
pub const GRID_COLS: usize = 148;
/// 网格行数（与 CPU 风格一致）
pub const GRID_ROWS: usize = 40;

/// 字形图集列数（覆盖 96 个 ASCII 可打印 + 1 个半块 ▌ + 余量）
const ATLAS_COLS: u32 = 16;
/// 字形图集行数
const ATLAS_ROWS: u32 = 7;
/// 图集单槽像素宽（输出 cell 的 2 倍，保证清晰）
const ATLAS_CELL_W: u32 = 20;
/// 图集单槽像素高
const ATLAS_CELL_H: u32 = 40;
/// CRT 移动高亮带速度（像素/帧）
const BAND_SPEED: f32 = 6.0;

/// 单网格单元在 GPU 侧的数据布局（16 字节，storage 数组无填充歧义）
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CellGpu {
    /// 字符码点（空格 32 表示空单元）
    pub ch: u32,
    /// 前景色，打包为 0xRRGGBB
    pub fg: u32,
    /// 背景色，打包为 0xRRGGBB
    pub bg: u32,
    /// 填充对齐
    pub _pad: u32,
}

/// 片元着色器 uniform（64 字节，16 字节对齐）
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    grid_cols: u32,
    grid_rows: u32,
    cell_w: f32,
    cell_h: f32,
    atlas_cols: u32,
    atlas_rows: u32,
    atlas_cw: f32,
    atlas_ch: f32,
    frame_w: f32,
    frame_h: f32,
    band_center: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
    _pad3: f32,
    _pad4: f32,
}

/// MidiConsole GPU 终端渲染器
pub struct MidiconsoleRenderer {
    pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    cells_buffer: wgpu::Buffer,
    output_texture: wgpu::Texture,
    output_texture_view: wgpu::TextureView,
    cell_w: f32,
    cell_h: f32,
    frame_w: u32,
    frame_h: u32,
}

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
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/midiconsole.wgsl").into()),
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

/// GPU 渲染上下文（封装 wgpu 设备/队列与渲染器），供导出路径在多次帧之间复用。
///
/// 所有 wgpu 类型均封闭在 `lumino_gfx` 内部，调用方（如导出 handler）无需直接依赖 wgpu。
pub struct MidiconsoleGpuContext {
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: MidiconsoleRenderer,
}

impl MidiconsoleGpuContext {
    /// 创建 GPU 上下文（无可用适配器时返回 `None`）。
    pub fn new(width: u32, height: u32) -> Option<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let adapter = futures::executor::block_on(
            instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
        )
        .ok()?;
        let (device, queue) =
            futures::executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: Some("midiconsole_gpu_export"),
                required_features: adapter.features() & wgpu::Features::default(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            }))
            .ok()?;
        let renderer = MidiconsoleRenderer::new(&device, &queue, width, height);
        Some(Self {
            device,
            queue,
            renderer,
        })
    }

    /// 渲染一帧并以 RGBA 字节返回。
    pub fn render_frame(&mut self, cells: &[CellGpu], tick: u32) -> Vec<u8> {
        self.renderer
            .render_to_rgba(&self.device, &self.queue, cells, tick)
    }
}

/// 加载等宽字体（与 CPU 预览一致：Consolas / DejaVu 候选）
fn load_monospace_font() -> Option<FontArc> {
    let candidates: Vec<PathBuf> = if cfg!(windows) {
        let dir =
            std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string()) + "\\Fonts\\";
        vec![
            PathBuf::from(dir.clone() + "consola.ttf"),
            PathBuf::from(dir.clone() + "consolab.ttf"),
            PathBuf::from(dir.clone() + "couri.ttf"),
            PathBuf::from(dir + "arial.ttf"),
        ]
    } else {
        vec![
            PathBuf::from("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
            PathBuf::from("/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
            PathBuf::from("/System/Library/Fonts/Supplemental/Courier New.ttf"),
            PathBuf::from("/Library/Fonts/Courier New.ttf"),
        ]
    };
    for p in &candidates {
        if let Ok(bytes) = std::fs::read(p)
            && let Ok(f) = FontVec::try_from_vec(bytes)
        {
            return Some(FontArc::from(f));
        }
    }
    None
}

/// 烘焙字形图集：ASCII 32..=126 + 半块 ▌(U+258C)，单槽 `ATLAS_CELL_W×ATLAS_CELL_H`，
/// 返回 (r8 覆盖率数据, 宽, 高)。无字体时返回空图集。
fn build_glyph_atlas() -> Option<(Vec<u8>, u32, u32)> {
    let font = load_monospace_font()?;
    let atlas_w = ATLAS_COLS * ATLAS_CELL_W;
    let atlas_h = ATLAS_ROWS * ATLAS_CELL_H;
    let mut data = vec![0u8; (atlas_w * atlas_h) as usize];

    let scale = PxScale::from(ATLAS_CELL_H as f32);
    let scaled = font.as_scaled(scale);
    let ascent = scaled.ascent();

    for slot in 0u32..(ATLAS_COLS * ATLAS_ROWS) {
        let ch: u32 = if slot < 95 {
            slot + 32
        } else if slot == 96 {
            0x258C
        } else {
            0
        };
        if ch == 0 {
            continue;
        }
        let gid = font.glyph_id(char::from_u32(ch)?);
        let glyph = gid.with_scale_and_position(scale, Point { x: 0.0, y: ascent });
        let Some(outline) = font.outline_glyph(glyph) else {
            continue;
        };
        let b = outline.px_bounds();
        let slot_x = (slot % ATLAS_COLS) * ATLAS_CELL_W;
        let slot_y = (slot / ATLAS_COLS) * ATLAS_CELL_H;
        outline.draw(|px, py, cov| {
            let x = (px as f32 + b.min.x).round() as i32 + slot_x as i32;
            let y = (py as f32 + b.min.y).round() as i32 + slot_y as i32;
            if x >= 0 && y >= 0 && x < atlas_w as i32 && y < atlas_h as i32 {
                let idx = (y as u32 * atlas_w + x as u32) as usize;
                data[idx] = (cov * 255.0).clamp(0.0, 255.0) as u8;
            }
        });
    }

    Some((data, atlas_w, atlas_h))
}

/// 把 `CellGpu` 单元数组打包为 0xRRGGBB 所需的小工具（供调用方复用）。
pub fn pack_rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

#[cfg(test)]
mod tests;
