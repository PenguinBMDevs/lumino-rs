//! 洋葱皮背景瓦片池
//!
//! 管理固定数量（`POOL_SIZE`）的离屏纹理，用于后台渲染洋葱皮背景瓦片。
//! 纹理预先分配，通过空闲链表 + 淘汰策略复用。

use iced_wgpu::wgpu;
use std::sync::Arc;

/// 洋葱皮背景瓦片元数据
#[derive(Debug, Clone, Copy)]
pub struct OnionBgTileMeta {
    /// 瓦片唯一标识
    pub tile_id: u64,
    /// 覆盖的 tick 范围 [start, end)
    pub tick_range: (f32, f32),
    /// 覆盖的 key 范围 [min, max]
    pub key_range: (u16, u16),
    /// LOD 层级（0=最精细，值越大越粗糙）
    pub lod: u8,
    /// 瓦片中包含的音符数量
    pub note_count: usize,
}

use wgpu::util::DeviceExt;

/// 洋葱皮背景瓦片池 — 固定 `POOL_SIZE` 块离屏纹理
///
/// 每块纹理 1024×512（匹配 LOD0 瓦片尺寸），Rgba8Unorm，支持 TEXTURE_BINDING。
/// 总内存：256 × 1024 × 512 × 4B = 512 MB。
const POOL_SIZE: usize = 256;
#[allow(dead_code)]
pub struct OnionBgTilePool {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    textures: Vec<wgpu::Texture>,
    texture_views: Vec<wgpu::TextureView>,
    in_use: [bool; POOL_SIZE],
    tile_metadata: [Option<OnionBgTileMeta>; POOL_SIZE],
}

impl OnionBgTilePool {
    /// 创建 256 块纹理，每块 1024×512，Rgba8Unorm
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let mut textures = Vec::with_capacity(POOL_SIZE);
        let mut texture_views = Vec::with_capacity(POOL_SIZE);

        for i in 0..256 {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some(&format!("onion_bg_tile_{}", i)),
                size: wgpu::Extent3d {
                    width: 1024,
                    height: 512,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            textures.push(tex);
            texture_views.push(view);
        }

        Self {
            device,
            queue,
            textures,
            texture_views,
            in_use: [false; 256],
            tile_metadata: [None; 256],
        }
    }

    /// 分配空闲块
    ///
    /// 有空闲时直接返回；无空闲时淘汰 lod=0 中 tile_id 最小的块。
    /// 返回 (块索引, 被淘汰的 tile_id)，无淘汰时第二个值为 None。
    /// 全部占用且无可淘汰时返回 `None`。
    pub fn alloc(&mut self) -> Option<(u16, Option<u64>)> {
        puffin::profile_function!();
        // 先找空闲块
        for (i, used) in self.in_use.iter().enumerate() {
            if !*used {
                self.in_use[i] = true;
                return Some((i as u16, None));
            }
        }

        // 无空闲，淘汰 lod=0 中最旧的块（按 tile_id 最小）
        let mut evict_idx = None;
        let mut oldest_id = u64::MAX;
        for (i, meta) in self.tile_metadata.iter().enumerate() {
            if let Some(m) = meta
                && m.lod == 0
                && m.tile_id < oldest_id
            {
                oldest_id = m.tile_id;
                evict_idx = Some(i);
            }
        }

        if let Some(idx) = evict_idx {
            let evicted_id = self.tile_metadata[idx].map(|m| m.tile_id);
            self.in_use[idx] = true;
            self.tile_metadata[idx] = None;
            return Some((idx as u16, evicted_id));
        }

        None
    }

    /// 释放指定块
    pub fn free(&mut self, index: u16) {
        let idx = index as usize;
        if idx < POOL_SIZE {
            self.in_use[idx] = false;
            self.tile_metadata[idx] = None;
        }
    }

    /// 获取指定块的纹理视图
    pub fn get_texture_view(&self, index: u16) -> &wgpu::TextureView {
        &self.texture_views[index as usize]
    }

    /// 获取指定块的纹理
    pub fn texture(&self, index: u16) -> &wgpu::Texture {
        &self.textures[index as usize]
    }

    /// 获取 GPU 队列引用
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// 获取 GPU 设备引用
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// 设置指定块的元数据
    pub fn set_metadata(&mut self, index: u16, meta: OnionBgTileMeta) {
        let idx = index as usize;
        if idx < POOL_SIZE {
            self.tile_metadata[idx] = Some(meta);
        }
    }

    /// 替换指定索引的纹理（使用 create_texture_with_data，确保正确布局转换）
    pub fn upload_texture(&mut self, index: u16, pixels: &[u8], width: u32, height: u32) {
        puffin::profile_scope!("upload_texture");
        let idx = index as usize;
        if idx >= POOL_SIZE {
            return;
        }
        tracing::info!(
            "[UPLOAD] pool_idx={} {}x{} pixels.len={}",
            index,
            width,
            height,
            pixels.len()
        );
        let new_tex = self.device.create_texture_with_data(
            &self.queue,
            &wgpu::TextureDescriptor {
                label: Some(&format!("onion_bg_tile_{}_upload", idx)),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::default(),
            pixels,
        );
        let new_view = new_tex.create_view(&wgpu::TextureViewDescriptor::default());
        self.textures[idx] = new_tex;
        self.texture_views[idx] = new_view;
    }
}
