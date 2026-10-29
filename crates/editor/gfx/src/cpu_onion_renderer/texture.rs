//! CPU 洋葱皮纹理资源：纹理 + 视图 + bind group（尺寸变化时整体重建）。

use crate::gpu_resource_tracker::TrackedTexture;

use super::{frame::CpuOnionFrame, pipeline::TEXTURE_FORMAT};

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
    /// 本步不做局部脏区；整帧一次 `write_texture`。
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
}
