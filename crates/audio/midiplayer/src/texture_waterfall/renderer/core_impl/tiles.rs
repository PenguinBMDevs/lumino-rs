//! 贴图生命周期管理（上传 / 移除 / 清空 / 查询）
//!
//! 拆分自 `core_impl.rs`，包含所有贴图（含临时脏区域覆层）的上传、
//! 移除、清空与查询方法。

use crate::texture_waterfall::renderer::texture::TileGpuResource;
use crate::texture_waterfall::renderer::uniform::TextureWaterfallUniform;
use crate::texture_waterfall::types::WaterfallTileCoord;

use super::TextureWaterfallRenderer;

impl TextureWaterfallRenderer {
    /// 上传一张贴图到 GPU
    pub fn upload_tile(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        coord: WaterfallTileCoord,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) {
        // 若已存在则先移除
        if self.tiles.contains_key(&coord) {
            self.remove_tile(&coord);
        }
        // 注意：不移除 dirty_overlays！这是有意为之——后台流式接收（GenerateTextureWaterfall
        // 或 RegenerateTextureWaterfallTrack）与 ShowTextureWaterfallDirtyOverlay 在同一帧循环中先后执行，
        // 若在此处清除覆层，新上传的覆盖层会在同一帧被后台贴图流误清除，导致用户
        // 永远看不到临时脏区域覆层。脏覆层在 upload_dirty_overlay 替换同坐标覆层时
        // 自然清理，或在 dispose_TextureWaterfall_onion_skin 全量释放。

        let byte_size = pixels.len();

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("texture_waterfall_{coord:?}")),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
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

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("texture_waterfall_uniform"),
            size: std::mem::size_of::<TextureWaterfallUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("texture_waterfall_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.tiles.insert(
            coord,
            TileGpuResource {
                texture,
                view,
                bind_group,
                uniform_buffer,
                byte_size,
            },
        );
        self.gpu_mem_used += byte_size;
        self.tile_order.push_back(coord);
        // 用户硬约束：不得限制 GPU 内存使用——删除 evict_if_over_limit 淘汰逻辑，
        // 所有上传的贴图常驻 GPU 显存，避免滚动到已淘汰时段时贴图瀑布流音符消失。
    }

    /// 移除一张贴图（释放显存）
    pub fn remove_tile(&mut self, coord: &WaterfallTileCoord) {
        if let Some(gpu) = self.tiles.remove(coord) {
            self.gpu_mem_used = self.gpu_mem_used.saturating_sub(gpu.byte_size);
        }
        self.tile_order.retain(|c| c != coord);
    }

    /// 清空所有贴图
    pub fn clear(&mut self) {
        self.tiles.clear();
        self.dirty_overlays.clear();
        self.gpu_mem_used = 0;
        self.tile_order.clear();
    }

    /// 清空指定音轨组的临时脏区域覆层
    pub fn clear_dirty_overlays(&mut self, track_group: u32) {
        self.dirty_overlays
            .retain(|coord, _| coord.track_group != track_group);
    }

    /// 上传一张临时脏区域贴图覆层
    pub fn upload_dirty_overlay(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        coord: WaterfallTileCoord,
        pixels: &[u8],
        width: u32,
        height: u32,
    ) {
        if let Some(gpu) = self.dirty_overlays.remove(&coord) {
            self.gpu_mem_used = self.gpu_mem_used.saturating_sub(gpu.byte_size);
        }

        let byte_size = pixels.len();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&format!("TextureWaterfall_dirty_overlay_{coord:?}")),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
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

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("TextureWaterfall_dirty_overlay_uniform"),
            size: std::mem::size_of::<TextureWaterfallUniform>() as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("TextureWaterfall_dirty_overlay_bind_group"),
            layout: &self.bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        self.dirty_overlays.insert(
            coord,
            TileGpuResource {
                texture,
                view,
                bind_group,
                uniform_buffer,
                byte_size,
            },
        );
        self.gpu_mem_used += byte_size;
    }

    /// 准备可见贴图的 uniform（在 render_pass 开始前调用）
    pub fn prepare(
        &self,
        queue: &wgpu::Queue,
        visible: &[(WaterfallTileCoord, TextureWaterfallUniform)],
    ) {
        for (coord, uniform) in visible {
            if let Some(gpu) = self.tiles.get(coord) {
                queue.write_buffer(&gpu.uniform_buffer, 0, bytemuck::bytes_of(uniform));
            }
        }
    }

    /// 检查贴图是否已上传
    pub fn has_tile(&self, coord: &WaterfallTileCoord) -> bool {
        self.tiles.contains_key(coord)
    }

    /// 检查临时脏区域覆层是否已上传
    pub fn has_dirty_overlay(&self, coord: &WaterfallTileCoord) -> bool {
        self.dirty_overlays.contains_key(coord)
    }

    /// 检查贴图或临时脏区域覆层是否已上传
    pub fn has_tile_or_dirty_overlay(&self, coord: &WaterfallTileCoord) -> bool {
        self.tiles.contains_key(coord) || self.dirty_overlays.contains_key(coord)
    }

    /// 已上传贴图数量
    pub fn tile_count(&self) -> usize {
        self.tiles.len()
    }

    /// 临时脏区域覆层数量
    pub fn dirty_overlay_count(&self) -> usize {
        self.dirty_overlays.len()
    }

    /// GPU 显存占用（字节）
    pub fn gpu_mem_used(&self) -> usize {
        self.gpu_mem_used
    }

    /// GPU 显存上限（字节）
    ///
    /// 用户硬约束：不得限制 GPU 内存使用。返回 usize::MAX 表示无限制。
    pub fn gpu_mem_limit(&self) -> usize {
        usize::MAX
    }

    /// 显存是否超限
    ///
    /// 用户硬约束：不得限制 GPU 内存使用。此函数始终返回 false，
    /// 保留方法以兼容外部查询接口（如统计面板显示）。
    pub fn is_over_limit(&self) -> bool {
        false
    }

    /// 显存淘汰逻辑（已禁用）
    ///
    /// 用户硬约束：不得限制 GPU 内存使用，不得淘汰已上传贴图。
    /// 保留为空实现以维持 API 兼容（外部可能有调用）。
    #[allow(dead_code)]
    fn evict_if_over_limit(&mut self) {
        // no-op：贴图常驻 GPU 显存
    }
}
