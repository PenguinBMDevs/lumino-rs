//! CPU 洋葱皮纹理资源：纹理 + 视图 + bind group（尺寸变化时整体重建）。

use crate::gpu_resource_tracker::TrackedTexture;

use super::{CpuOnionUploadRect, frame::CpuOnionFrame, pipeline::TEXTURE_FORMAT};

/// 驻留纹理与其 bind group。
///
/// bind group 同时绑定 uniform（binding 0）、纹理视图（binding 1）与采样器
///（binding 2），因此纹理尺寸变化时 bind group 必须随之重建。
pub(super) struct CpuOnionTexture {
    texture: TrackedTexture,
    bind_group: wgpu::BindGroup,
    size: (u32, u32),
}

impl CpuOnionTexture {
    /// 创建纹理与配套 bind group。
    ///
    /// # 参数
    /// * `layout` — bind group layout
    /// * `sampler` — 渲染器持有的最近邻采样器
    /// * `uniform` — 渲染器持有的 uniform 缓冲（uniform 内容独立更新，无需重建）
    pub(super) fn create(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        sampler: &wgpu::Sampler,
        uniform: &wgpu::Buffer,
        width: u32,
        height: u32,
    ) -> Self {
        let texture = TrackedTexture::new(
            device,
            &wgpu::TextureDescriptor {
                label: Some("cpu_onion_texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: TEXTURE_FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cpu_onion_bind_group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
            ],
        });

        Self {
            texture,
            bind_group,
            size: (width, height),
        }
    }

    /// 驻留纹理尺寸（物理像素）。
    pub(super) fn size(&self) -> (u32, u32) {
        self.size
    }

    /// bind group（binding 0/1/2 的完整绑定）。
    pub(super) fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }

    /// 全量上传一帧像素（调用方需保证 `frame.validate()` 成立）。
    ///
    /// 用于「新纹理首次铺满」与「脏区不可信」两条保守路径；区域重绘走
    /// [`Self::upload_region`]。
    pub(super) fn upload(&self, queue: &wgpu::Queue, frame: &CpuOnionFrame) {
        let (width, height) = self.size;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: self.texture.inner(),
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            frame.pixels.as_ref(),
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

    /// 局部上传：只重写 `rect` 覆盖的纹理子矩形，**其余像素保持上一版**。
    ///
    /// 目标 origin = `(rect.x, rect.y, 0)`，extent = `(rect.width, rect.height, 1)`；
    /// 源数据从 `(rect.y * frame.width + rect.x) * 4` 起，行距固定为
    /// `frame.width * 4`（源是整帧像素，不是紧凑子图）。
    ///
    /// 返回 `false` 表示**没有写入**（矩形非法 / 越界 / 帧像素不足），调用方应
    /// 复位上传状态并在下一帧走全量重建——本函数绝不 panic。
    pub(super) fn upload_region(
        &self,
        queue: &wgpu::Queue,
        frame: &CpuOnionFrame,
        rect: CpuOnionUploadRect,
    ) -> bool {
        if self.size != frame.size() {
            return false;
        }
        let Some((start, end)) = frame.region_span(rect) else {
            return false;
        };
        let Some(pixels) = frame.pixels.get(start..end) else {
            return false;
        };
        let Some(bytes_per_row) = frame.width.checked_mul(4) else {
            return false;
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: self.texture.inner(),
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: rect.x,
                    y: rect.y,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                // 子矩形只有一层：行距已由 `bytes_per_row` 表达，无需行图间距
                rows_per_image: None,
            },
            wgpu::Extent3d {
                width: rect.width,
                height: rect.height,
                depth_or_array_layers: 1,
            },
        );
        true
    }
}
