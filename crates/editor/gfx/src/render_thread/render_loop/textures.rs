use std::sync::{Arc, Mutex};

use super::super::params::RenderParams;
use crate::gpu_resource_tracker::TrackedTexture;

/// 离屏纹理资源集合
pub struct OffscreenTextureResources<'a> {
    pub device: &'a wgpu::Device,
    pub texture_format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pub current_size: &'a mut (u32, u32),
    pub current_texture: &'a mut Option<Arc<TrackedTexture>>,
    pub depth_texture: &'a mut Option<TrackedTexture>,
    pub depth_texture_view: &'a mut Option<wgpu::TextureView>,
    pub texture_view: &'a mut Option<wgpu::TextureView>,
    pub latest_texture_clone: &'a Arc<Mutex<Option<Arc<TrackedTexture>>>>,
    pub params: &'a RenderParams,
}

/// 确保离屏纹理已创建并返回
///
/// `needs_depth` 控制是否创建深度纹理；视频导出为纯 2D 渲染，可跳过 depth。
pub fn ensure_textures(resources: &mut OffscreenTextureResources<'_>, needs_depth: bool) -> bool {
    let width = resources.width.max(1);
    let height = resources.height.max(1);

    // 如果尺寸改变或纹理不存在，重新创建
    let needs_recreate = *resources.current_size != (width, height)
        || resources.current_texture.is_none()
        || resources.texture_view.is_none()
        || (needs_depth && resources.depth_texture.is_none());
    if needs_recreate {
        // 先释放旧视图（视图不计入独立内存，但需在其父纹理之前释放）
        if resources.texture_view.is_some() {
            resources.texture_view.take();
        }
        if resources.depth_texture_view.is_some() {
            resources.depth_texture_view.take();
        }
        // 旧纹理由 Option::take 触发 TrackedBuffer Drop 自动注销内存计数
        resources.depth_texture.take();
        resources.current_texture.take();

        // 创建离屏渲染纹理
        let texture = TrackedTexture::new(
            resources.device,
            &wgpu::TextureDescriptor {
                label: Some("offscreen_render_texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: resources.texture_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            },
        );

        *resources.texture_view =
            Some(texture.create_view(&wgpu::TextureViewDescriptor::default()));

        // 按需创建深度纹理
        if needs_depth {
            let depth_tex = TrackedTexture::new(
                resources.device,
                &wgpu::TextureDescriptor {
                    label: Some("depth_texture"),
                    size: wgpu::Extent3d {
                        width,
                        height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Depth32Float,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                },
            );

            resources
                .depth_texture_view
                .replace(depth_tex.create_view(&wgpu::TextureViewDescriptor::default()));
            *resources.depth_texture = Some(depth_tex);
        }

        *resources.current_texture = Some(Arc::new(texture));
        *resources.current_size = (width, height);

        // 将新纹理共享给主线程
        if let Ok(mut lock) = resources.latest_texture_clone.lock() {
            *lock = resources.current_texture.clone();
        }
        return true;
    }
    false
}
