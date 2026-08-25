use std::sync::{Arc, Mutex};

use super::super::params::RenderParams;
use crate::gpu_resource_tracker::TrackedTexture;

/// 离屏纹理资源集合
pub struct OffscreenTextureResources<'a> {
    /// 逻辑设备（用于创建纹理）
    pub device: &'a wgpu::Device,
    /// 渲染目标纹理格式
    pub texture_format: wgpu::TextureFormat,
    /// 离屏纹理宽度（像素）
    pub width: u32,
    /// 离屏纹理高度（像素）
    pub height: u32,
    /// 当前视口尺寸（创建尺寸变化时更新）
    pub current_size: &'a mut (u32, u32),
    /// 当前帧离屏渲染纹理
    pub current_texture: &'a mut Option<Arc<TrackedTexture>>,
    /// 当前帧深度纹理
    pub depth_texture: &'a mut Option<TrackedTexture>,
    /// 当前帧深度纹理视图
    pub depth_texture_view: &'a mut Option<wgpu::TextureView>,
    /// 当前帧离屏纹理视图（缓存避免每帧 create_view）
    pub texture_view: &'a mut Option<wgpu::TextureView>,
    /// 最新纹理共享引用（渲染线程 → 主线程）
    pub latest_texture_clone: &'a Arc<Mutex<Option<Arc<TrackedTexture>>>>,
    /// 当前帧渲染参数
    pub params: &'a RenderParams,
}

/// 确保离屏纹理已创建并返回
///
/// `needs_depth` 控制是否创建深度纹理；视频导出为纯 2D 渲染，可跳过 depth。
pub fn ensure_textures(resources: &mut OffscreenTextureResources<'_>, needs_depth: bool) -> bool {
    // macOS 最大化时物理尺寸可达 10k+（Retina 2x），超 8192 硬上限会触发
    // device lost/黑屏（参考 yinhe render_context.rs:181）。此处同 yinhe 做上限裁剪。
    let max_dim = resources.device.limits().max_texture_dimension_2d;
    let width = resources.width.min(max_dim).max(1);
    let height = resources.height.min(max_dim).max(1);

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

        // sRGB 需提供 linear view_format（Metal/Vulkan 在 TEXTURE_BINDING 时校验）
        // 缺失会导致最大化时大纹理创建失败进而 device lost（yinhe:183）
        let linear_format = match resources.texture_format {
            wgpu::TextureFormat::Bgra8UnormSrgb => Some(wgpu::TextureFormat::Bgra8Unorm),
            wgpu::TextureFormat::Rgba8UnormSrgb => Some(wgpu::TextureFormat::Rgba8Unorm),
            _ => None,
        };
        let view_formats: &[wgpu::TextureFormat] = if let Some(lf) = &linear_format {
            std::slice::from_ref(lf)
        } else {
            &[]
        };

        // 创建离屏渲染纹理（需 TEXTURE_BINDING 供最大化/拖拽 resize 时 blit 拉伸，避免黑边）
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
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats,
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
