use std::sync::Arc;

use iced_wgpu::wgpu;

use super::{KEY_COUNT_MAX, KeyboardRenderer};

impl KeyboardRenderer {
    /// 确保活跃键颜色缓冲存在（KEY_COUNT_MAX × u32，storage + 每帧清零用 COPY_DST）
    pub(super) fn ensure_key_colors(&mut self, device: &wgpu::Device) {
        if self.key_colors.is_some() {
            return;
        }
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("piano_waterfall_key_colors"),
            size: (KEY_COUNT_MAX as u64) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.key_colors = Some(buf);
    }

    /// 确保离屏纹理尺寸匹配（跨帧复用，仅在尺寸变化时重建）；其视图交给 iced shader 图元采样。
    pub(super) fn ensure_targets(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        if self.last_w == width && self.last_h == height && self.tex.is_some() {
            return;
        }
        let tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("piano_waterfall_tex"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            // RENDER_ATTACHMENT：离屏渲染目标；TEXTURE_BINDING：iced shader 图元采样合成。
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let tex_view = Arc::new(tex.create_view(&wgpu::TextureViewDescriptor::default()));
        self.tex = Some(tex);
        self.tex_view = Some(tex_view);
        self.last_w = width;
        self.last_h = height;
    }

    /// 确保可见索引缓冲容量匹配音符数（仅在数量变化时重建）
    pub(super) fn ensure_visible_indices(&mut self, device: &wgpu::Device, count: u32) {
        if self.last_count == count && count > 0 {
            return;
        }
        let size = (count.max(1) as u64) * 4;
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("piano_waterfall_visible_indices"),
            size,
            usage: wgpu::BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        self.visible_indices = buf;
        self.last_count = count;
    }
}
