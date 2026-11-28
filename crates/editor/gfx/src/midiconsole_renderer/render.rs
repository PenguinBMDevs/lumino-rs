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
        let (atlas_view, sampler) = Self::build_atlas(device, queue);

        // —— 单元 / uniform 缓冲 ——
        let cell_count = GRID_COLS * GRID_ROWS;
        let cells_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("midiconsole_cells"),
            size: (cell_count * std::mem::size_of::<CellGpu>()) as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let uniform_buffer = Self::create_uniform_buffer(device, cell_w, cell_h, width, height);

        // —— 绑定组布局 ——
        let (bind_group_layout, bind_group) = Self::build_bindings(
            device,
            &uniform_buffer,
            &atlas_view,
            &sampler,
            &cells_buffer,
        );

        let pipeline = crate::pipeline::RenderPipelineBuilder::new(
            device,
            "midiconsole_pipeline",
            &shader_module,
        )
        .bind_group(&bind_group_layout)
        // 导出编码器按 "bgra" 消费：让 GPU 直接产出 BGRA，CPU 侧零 swizzle。
        .opaque_target(wgpu::TextureFormat::Bgra8Unorm)
        .build();

        let (output_texture, output_texture_view) =
            Self::create_output_texture(device, width, height);

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
            staging: None,
        }
    }

    /// 烘焙字形图集并创建采样器。
    fn build_atlas(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> (wgpu::TextureView, wgpu::Sampler) {
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
        (atlas_view, sampler)
    }

    /// 创建 uniform 缓冲（内容为创建时的一次性参数）。
    fn create_uniform_buffer(
        device: &wgpu::Device,
        cell_w: f32,
        cell_h: f32,
        width: u32,
        height: u32,
    ) -> wgpu::Buffer {
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
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("midiconsole_uniform"),
            contents: bytemuck::bytes_of(&uniforms),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }

    /// 创建绑定组布局与绑定组。
    fn build_bindings(
        device: &wgpu::Device,
        uniform_buffer: &wgpu::Buffer,
        atlas_view: &wgpu::TextureView,
        sampler: &wgpu::Sampler,
        cells_buffer: &wgpu::Buffer,
    ) -> (wgpu::BindGroupLayout, wgpu::BindGroup) {
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
                    resource: wgpu::BindingResource::TextureView(atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: cells_buffer.as_entire_binding(),
                },
            ],
        });

        (bind_group_layout, bind_group)
    }

    /// 创建离屏输出纹理（BGRA，导出编码器直接消费）。
    fn create_output_texture(
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> (wgpu::Texture, wgpu::TextureView) {
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
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let output_texture_view =
            output_texture.create_view(&wgpu::TextureViewDescriptor::default());
        (output_texture, output_texture_view)
    }

    /// 渲染一帧并以 **RGBA** 字节返回（未做行对齐 padding）。
    ///
    /// **仅测试/预览用**：GPU 输出为 BGRA（编码器消费格式），本方法额外做一次
    /// 通道交换。生产导出走 [`Self::render_to_bgra_into`]：零中间 Vec、零 swizzle。
    ///
    /// 网格数量不符或 GPU 读回失败时返回 `None`（调用方回退 CPU 路径），不 panic。
    pub fn render_to_rgba(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cells: &[CellGpu],
        tick: u32,
    ) -> Option<Vec<u8>> {
        self.render_frame(device, queue, cells, tick)?;
        let mut out = vec![0u8; (self.frame_w * self.frame_h * 4) as usize];
        if !self.readback_into(device, &mut out) {
            return None;
        }
        // BGRA → RGBA（兼容 API）
        for px in out.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        Some(out)
    }

    /// 渲染一帧并**直接写入调用方 BGRA 缓冲**（生产路径）。
    ///
    /// `dst.len()` 必须 ≥ `frame_w * frame_h * 4`；返回 `false` 表示网格数量
    /// 不符或 GPU 读回失败（调用方回退 CPU 路径）。1080p 等 256 对齐宽度走
    /// 整块 memcpy，其余分辨率逐行 memcpy——不再有逐像素循环。
    pub fn render_to_bgra_into(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cells: &[CellGpu],
        tick: u32,
        dst: &mut [u8],
    ) -> bool {
        let need = (self.frame_w * self.frame_h * 4) as usize;
        if dst.len() < need {
            return false;
        }
        if self.render_frame(device, queue, cells, tick).is_none() {
            return false;
        }
        self.readback_into(device, &mut dst[..need])
    }

    /// 提交渲染并把结果拷入 staging（不读回）。
    fn render_frame(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cells: &[CellGpu],
        tick: u32,
    ) -> Option<()> {
        Self::check_cells(cells)?;

        self.upload_frame_data(queue, cells, tick);

        let mut encoder = self.begin_scene(device);

        self.stage_readback(device, &mut encoder)?;
        queue.submit(std::iter::once(encoder.finish()));
        Some(())
    }

    /// 校验网格单元数量与 `GRID_COLS × GRID_ROWS` 一致。
    fn check_cells(cells: &[CellGpu]) -> Option<()> {
        let cell_count = GRID_COLS * GRID_ROWS;
        if cells.len() != cell_count {
            tracing::warn!(
                "midiconsole 网格单元数量不符：期望 {}，实际 {}",
                cell_count,
                cells.len()
            );
            return None;
        }
        Some(())
    }

    /// 上传本帧 uniform 与网格单元。
    fn upload_frame_data(&self, queue: &wgpu::Queue, cells: &[CellGpu], tick: u32) {
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
    }

    /// 录制清屏 + 全屏三角形绘制，返回待提交的命令编码器。
    fn begin_scene(&self, device: &wgpu::Device) -> wgpu::CommandEncoder {
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
        encoder
    }

    /// 确保 staging 容量足够，并录制纹理 → 缓冲拷贝。
    fn stage_readback(
        &mut self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Option<()> {
        // GPU → CPU 读回：staging 跨帧复用（旧实现每帧 create_buffer_init 8MB
        // 零值并上传，随后立刻被 copy 覆盖——纯浪费带宽/分配）。
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let bpp = 4u32;
        let padded = (self.frame_w * bpp).next_multiple_of(align);
        let need = (padded * self.frame_h) as u64;
        let reusable = self.staging.as_ref().is_some_and(|b| b.size() >= need);
        if !reusable {
            self.staging = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("midiconsole_staging"),
                size: need,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            }));
        }
        let staging = self.staging.as_ref()?;
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &self.output_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: staging,
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
        Some(())
    }

    /// 把 staging 内容读入 `dst`（去掉行对齐 padding），含 TDR 防护。
    fn readback_into(&self, device: &wgpu::Device, dst: &mut [u8]) -> bool {
        let Some(staging) = self.staging.as_ref() else {
            return false;
        };
        let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
        let unpadded = (self.frame_w * 4) as usize;
        let padded = unpadded.next_multiple_of(align);

        let slice = staging.slice(..);
        // REND-019 PR-D（R5-b 实测崩溃根因一）：device 丢失（TDR）时禁止发起
        // 映射——map 回调可能永不送达，残留 `mapped_range` 会毒化后续
        // `map_async`（wgpu 断言「Buffer is already mapped」）。
        if crate::device_status::device_lost() {
            tracing::warn!("midiconsole 读回跳过：device 已丢失（TDR）");
            return false;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        // 有界等待（禁止 timeout: None 无限阻塞）：轮询完成回调，超时由下方
        // `recv_timeout` 明确失败，不挂死调用线程。
        let _ = device.poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(30)),
        });
        let Ok(callback) = rx.recv_timeout(std::time::Duration::from_secs(30)) else {
            // 超时必须 unmap 复位映射状态（否则下次 map_async 断言崩溃）
            crate::device_status::safe_unmap(staging);
            tracing::warn!("midiconsole 读回超时（map_async 回调未收到）");
            return false;
        };
        if callback.is_err() {
            crate::device_status::safe_unmap(staging);
            tracing::warn!("midiconsole 读回失败（map_async 返回错误）");
            return false;
        }

        // REND-019（R5-b 实测崩溃根因三）：回调 Ok 与 destroy 的竞态窗口内
        // get_mapped_range 走 facade panic——catch 后按读回失败处理。
        let mapped =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| slice.get_mapped_range()));
        let Ok(data) = mapped else {
            crate::device_status::safe_unmap(staging);
            tracing::warn!("midiconsole 读回竞态（缓冲已随 TDR 销毁），放弃本帧");
            return false;
        };

        // 行对齐裁剪：1080p 等 256 对齐宽度（1920×4=7680）走整块 memcpy，
        // 其余分辨率逐行 memcpy（1920 次，而非旧实现的 200 万次逐像素拷贝）。
        if padded == unpadded {
            let need = unpadded * self.frame_h as usize;
            dst.copy_from_slice(&data[..need]);
        } else {
            for y in 0..self.frame_h as usize {
                let src = y * padded;
                let di = y * unpadded;
                dst[di..di + unpadded].copy_from_slice(&data[src..src + unpadded]);
            }
        }
        drop(data);
        crate::device_status::safe_unmap(staging);
        true
    }
}
