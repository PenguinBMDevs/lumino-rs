//! 洋葱皮位图管理器
//!
//! 将每个音轨的音符预渲染为独立位图（GPU 纹理），
//! 在主渲染通道中以全屏纹理四边形展示。
//!
//! 位图生成流程：
//! 1. CPU 端将音符变换到屏幕坐标 → 填充像素缓冲区
//! 2. 上传到 GPU 纹理（每个音轨独立纹理）
//! 3. 主渲染通道中，每个激活音轨绘制一个全屏纹理四边形
//! 4. 当前音轨跳过不显示
//!
//! 位图仅在视口变化或音轨数据变化时重新生成。

use iced_wgpu::wgpu;

/// 视口信息，用于将音符逻辑坐标变换到屏幕像素坐标
#[derive(Debug, Clone, Copy)]
pub struct BitmapViewport {
    pub scroll_x: f32,
    pub scroll_y: f32,
    pub zoom_x: f32,
    pub zoom_y: f32,
    pub keyboard_width: f32,
    pub ruler_height: f32,
    pub max_key_index: f32,
    pub canvas_offset_x: f32,
    pub canvas_offset_y: f32,
    pub physical_width: u32,
    pub physical_height: u32,
    /// 缩放因子（视网膜屏 > 1.0，用于 logical → physical 坐标变换）
    pub scale: f32,
}

impl Default for BitmapViewport {
    fn default() -> Self {
        Self {
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom_x: 1.0,
            zoom_y: 1.0,
            keyboard_width: 60.0,
            ruler_height: 30.0,
            max_key_index: 127.0,
            canvas_offset_x: 0.0,
            canvas_offset_y: 0.0,
            physical_width: 1,
            physical_height: 1,
            scale: 1.0,
        }
    }
}

impl BitmapViewport {
    /// 计算视口哈希，用于检测视口变化
    pub fn hash(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.scroll_x.to_bits().hash(&mut hasher);
        self.scroll_y.to_bits().hash(&mut hasher);
        self.zoom_x.to_bits().hash(&mut hasher);
        self.zoom_y.to_bits().hash(&mut hasher);
        self.physical_width.hash(&mut hasher);
        self.physical_height.hash(&mut hasher);
        self.scale.to_bits().hash(&mut hasher);
        hasher.finish()
    }

    /// 将音符逻辑坐标 (tick, key) 变换为屏幕物理像素坐标
    /// 输出经过 scale 缩放以匹配物理分辨率
    fn note_to_screen(&self, tick: f32, key: f32, length: f32) -> (f32, f32, f32, f32) {
        let scale = self.scale;
        let x = (tick * self.zoom_x - self.scroll_x + self.keyboard_width + self.canvas_offset_x)
            * scale;
        let y = ((self.max_key_index - key) * self.zoom_y - self.scroll_y
            + self.ruler_height
            + self.canvas_offset_y)
            * scale;
        let w = length * self.zoom_x * scale;
        let h = self.zoom_y * scale;
        (x, y, w, h)
    }
}

/// 单个音轨的位图缓存
pub(crate) struct TrackBitmap {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    /// 是否需要重新生成位图
    pub is_dirty: bool,
}

/// 洋葱皮位图管理器
///
/// 管理每个音轨的位图生成、缓存和展示。
/// 位图在音轨数据变化或视口变化时重新生成。
pub struct OnionSkinBitmapManager {
    /// 按音轨索引存储的位图
    track_bitmaps: Vec<Option<TrackBitmap>>,

    /// 纹理展示管线（在主渲染通道中绘制全屏四边形）
    display_pipeline: wgpu::RenderPipeline,
    /// 纹理展示的 bind group layout
    display_bind_group_layout: wgpu::BindGroupLayout,
    /// 纹理采样器
    sampler: wgpu::Sampler,

    /// 当前视口（用于判断是否需要重新生成）
    viewport: BitmapViewport,
    /// 上次生成时的视口哈希
    last_viewport_hash: u64,
}

impl OnionSkinBitmapManager {
    /// 全屏四边形的 WGSL 着色器（顶点 + 片段）
    /// 顶点输出 UV 坐标用于纹理采样
    const DISPLAY_SHADER: &'static str = r#"
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
    "#;

    /// 创建新的洋葱皮位图管理器
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        viewport: BitmapViewport,
    ) -> Self {
        // 创建纹理采样器（双线性过滤，用于平滑缩放）
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("onion_skin_sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        // 创建展示管线的 bind group layout
        let display_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("onion_skin_display_bind_group_layout"),
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

        // 编译展示着色器（包含顶点和片段）
        let display_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("onion_skin_display_shader"),
            source: wgpu::ShaderSource::Wgsl(Self::DISPLAY_SHADER.into()),
        });

        // 创建展示管线布局
        let display_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("onion_skin_display_pipeline_layout"),
                bind_group_layouts: &[&display_bind_group_layout],
                push_constant_ranges: &[],
            });

        // 创建展示管线
        let display_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("onion_skin_display_pipeline"),
            layout: Some(&display_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &display_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &display_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
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

        let vp_hash = viewport.hash();
        Self {
            track_bitmaps: Vec::new(),
            display_pipeline,
            display_bind_group_layout,
            sampler,
            viewport,
            last_viewport_hash: vp_hash,
        }
    }

    /// 确保音轨位图槽位存在
    fn ensure_track_slot(&mut self, track_idx: usize) {
        if track_idx >= self.track_bitmaps.len() {
            self.track_bitmaps.resize_with(track_idx + 1, || None);
        }
    }

    /// 检查视口是否变化，若变化则标记所有位图为脏
    pub fn check_viewport_changed(&mut self, viewport: &BitmapViewport) {
        let new_hash = viewport.hash();
        if new_hash != self.last_viewport_hash {
            self.last_viewport_hash = new_hash;
            self.viewport = *viewport;
            // 视口变化，标记所有位图为脏
            for slot in &mut self.track_bitmaps {
                if let Some(tb) = slot {
                    tb.is_dirty = true;
                }
            }
        }
    }

    /// 生成或更新指定音轨的位图
    ///
    /// # 参数
    /// * `device` - WGPU 设备
    /// * `queue` - WGPU 队列
    /// * `track_idx` - 音轨索引
    /// * `notes` - 音符数据：(tick, key, length) 列表
    pub fn generate_track_bitmap(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        track_idx: usize,
        color: [f32; 4],
        notes: &[(f32, u16, f32)],
    ) {
        self.ensure_track_slot(track_idx);

        let width = self.viewport.physical_width.max(1);
        let height = self.viewport.physical_height.max(1);

        // 计算像素缓冲区大小
        let pixel_count = (width * height) as usize;
        let mut pixels = vec![0u8; pixel_count * 4]; // RGBA

        // 将颜色转为 RGBA8
        let r = (color[0].clamp(0.0, 1.0) * 255.0) as u8;
        let g = (color[1].clamp(0.0, 1.0) * 255.0) as u8;
        let b = (color[2].clamp(0.0, 1.0) * 255.0) as u8;
        let a = (color[3].clamp(0.0, 1.0) * 255.0) as u8;

        // 对每个音符，在像素缓冲区中绘制矩形
        for &(tick, key, length) in notes {
            let (sx, sy, sw, sh) = self.viewport.note_to_screen(tick, key as f32, length);

            // 裁剪到视口范围
            let x0 = sx.max(0.0) as u32;
            let y0 = sy.max(0.0) as u32;
            let x1 = ((sx + sw).min(width as f32)) as u32;
            let y1 = ((sy + sh).min(height as f32)) as u32;

            if x0 >= x1 || y0 >= y1 {
                continue;
            }

            // 逐行填充矩形
            for py in y0..y1 {
                let row_start = (py * width * 4) as usize;
                let start_idx = row_start + (x0 * 4) as usize;
                let end_idx = row_start + (x1 * 4) as usize;
                let row_slice = &mut pixels[start_idx..end_idx];

                // 按 4 字节块写入颜色
                for chunk in row_slice.chunks_exact_mut(4) {
                    chunk[0] = r;
                    chunk[1] = g;
                    chunk[2] = b;
                    chunk[3] = a;
                }
            }
        }

        // 检查是否需要创建新纹理
        let recreate = match self.track_bitmaps[track_idx].as_ref() {
            Some(tb) => tb.width != width || tb.height != height,
            None => true,
        };

        if recreate {
            // 创建新的 GPU 纹理
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("onion_skin_track_{}", track_idx)),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.track_bitmaps[track_idx] = Some(TrackBitmap {
                texture,
                view,
                width,
                height,
                is_dirty: false,
            });
        }

        // 上传像素数据到 GPU
        if let Some(Some(tb)) = self.track_bitmaps.get(track_idx) {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &tb.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(width * 4),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
        }

        // 清除脏标记
        if let Some(Some(tb)) = self.track_bitmaps.get_mut(track_idx) {
            tb.is_dirty = false;
        }
    }

    /// 在主渲染通道中绘制洋葱皮位图
    ///
    /// # 参数
    /// * `render_pass` - 主渲染通道
    /// * `active_tracks` - 需要显示的音轨列表（不包含当前音轨）
    /// * `device` - WGPU 设备
    pub fn display_bitmaps<'r>(
        &'r self,
        render_pass: &mut wgpu::RenderPass<'r>,
        active_tracks: &[usize],
        device: &wgpu::Device,
    ) {
        for &track_idx in active_tracks {
            let Some(Some(tb)) = self.track_bitmaps.get(track_idx) else {
                continue;
            };
            if tb.is_dirty {
                continue; // 脏位图跳过显示（等下次生成后自动更新）
            }

            // 创建绑定组：纹理 + 采样器
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some(&format!("onion_skin_display_bg_{}", track_idx)),
                layout: &self.display_bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&tb.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });

            render_pass.set_pipeline(&self.display_pipeline);
            render_pass.set_bind_group(0, &bind_group, &[]);
            render_pass.draw(0..4, 0..1);
        }
    }

    /// 清空所有位图（当 MIDI 文件关闭时）
    pub fn clear(&mut self) {
        self.track_bitmaps.clear();
        self.last_viewport_hash = 0;
    }

    /// 获取脏位图数量
    pub fn dirty_count(&self) -> usize {
        self.track_bitmaps
            .iter()
            .filter_map(|s| s.as_ref())
            .filter(|tb| tb.is_dirty)
            .count()
    }

    /// 获取所有脏音轨索引
    pub fn dirty_tracks(&self) -> Vec<usize> {
        self.track_bitmaps
            .iter()
            .enumerate()
            .filter_map(|(idx, slot)| {
                if let Some(tb) = slot {
                    if tb.is_dirty {
                        return Some(idx);
                    }
                }
                None
            })
            .collect()
    }

    /// 获取指定音轨集合的纹理视图（用于发送到渲染线程）
    /// 返回 (texture_views, sampler)
    pub fn collect_views(
        &self,
        track_indices: &[usize],
    ) -> (Vec<wgpu::TextureView>, Option<wgpu::Sampler>) {
        let views: Vec<wgpu::TextureView> = track_indices
            .iter()
            .filter_map(|&idx| {
                self.track_bitmaps
                    .get(idx)
                    .and_then(|s| s.as_ref())
                    .filter(|tb| !tb.is_dirty)
                    .map(|tb| tb.view.clone())
            })
            .collect();
        let sampler = (!views.is_empty()).then(|| self.sampler.clone());
        (views, sampler)
    }
}
