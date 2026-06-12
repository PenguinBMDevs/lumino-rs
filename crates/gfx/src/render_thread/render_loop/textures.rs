use std::sync::{Arc, Mutex};

use super::super::params::RenderParams;

/// 确保离屏纹理已创建并返回
pub fn ensure_textures(
    device: &wgpu::Device,
    texture_format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    current_size: &mut (u32, u32),
    current_texture: &mut Option<Arc<wgpu::Texture>>,
    depth_texture: &mut Option<wgpu::Texture>,
    depth_texture_view: &mut Option<wgpu::TextureView>,
    latest_texture_clone: &Arc<Mutex<Option<Arc<wgpu::Texture>>>>,
    _params: &RenderParams,
) -> bool {
    let width = width.max(1);
    let height = height.max(1);

    // 如果尺寸改变或纹理不存在，重新创建
    if *current_size != (width, height) || current_texture.is_none() || depth_texture.is_none() {
        // 创建离屏渲染纹理
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen_render_texture"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: texture_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        // 创建深度纹理
        let depth_tex = device.create_texture(&wgpu::TextureDescriptor {
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
        });

        depth_texture_view.replace(depth_tex.create_view(&wgpu::TextureViewDescriptor::default()));
        *depth_texture = Some(depth_tex);
        *current_texture = Some(Arc::new(texture));
        *current_size = (width, height);

        // 将新纹理共享给主线程
        if let Ok(mut lock) = latest_texture_clone.lock() {
            *lock = current_texture.clone();
        }
        return true;
    }
    false
}
