//! 纹理上传与外部纹理绑定（字体图集 / 用户纹理 / 离屏渲染产物）。

use egui::TextureId;
use egui::TexturesDelta;
use egui::epaint::ImageData;

use super::Painter;

/// 已上传的 egui 纹理。
pub(super) struct TextureEntry {
    /// 纹理本体（`bind_group` 内部也会持有引用，这里保留便于分块更新）。
    pub(super) texture: wgpu::Texture,
    pub(super) bind_group: wgpu::BindGroup,
}

/// 外部纹理（离屏渲染产物）：本层只建绑定组，纹理由调用方持有并更新。
pub(super) struct ExternalTexture {
    /// 视图指针（`Arc::as_ptr`）：同一指针 = 同一张纹理，免去每帧重建绑定组。
    pub(super) key: usize,
    pub(super) bind_group: wgpu::BindGroup,
}

impl Painter {
    /// 注册外部纹理（离屏渲染产物，如钢琴瀑布流）。
    ///
    /// 同一个 `Arc<TextureView>` 指针视为同一张纹理：重复注册是空操作，
    /// 离屏纹理重建（尺寸变化）产生新视图时自动重建绑定组。
    pub fn set_external_texture(
        &mut self,
        device: &wgpu::Device,
        id: TextureId,
        view: &std::sync::Arc<wgpu::TextureView>,
    ) {
        let key = std::sync::Arc::as_ptr(view) as usize;
        if self.external.get(&id).is_some_and(|entry| entry.key == key) {
            return;
        }
        let label = format!("lumino_egui_external_{id:?}");
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&label),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.external
            .insert(id, ExternalTexture { key, bind_group });
    }

    /// 注销外部纹理（面板关闭 / 纹理失效）。
    pub fn clear_external_texture(&mut self, id: TextureId) {
        self.external.remove(&id);
    }

    /// 应用 egui 的纹理增量（字体图集 / 用户纹理）。
    ///
    /// 与 `egui-wgpu` 的契约一致：调用方把 [`TexturesDelta`] 交出来，
    /// 本方法负责全部应用并清空（未应用的增量在析构时会 panic）。
    pub fn update_textures(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        delta: &mut TexturesDelta,
    ) {
        for (id, deltas) in &delta.set {
            for image_delta in deltas {
                let [image_width, image_height] = image_delta.image.size();
                let width = image_width.max(1) as u32;
                let height = image_height.max(1) as u32;
                let ImageData::Color(image) = &image_delta.image;
                let pixels: Vec<u8> = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect();

                match image_delta.pos {
                    None => {
                        let entry = self.create_texture(device, queue, *id, width, height, &pixels);
                        self.textures.insert(*id, entry);
                    }
                    Some([x, y]) => {
                        if let Some(entry) = self.textures.get(id) {
                            write_texture(
                                queue,
                                &entry.texture,
                                [x as u32, y as u32],
                                width,
                                height,
                                &pixels,
                            );
                        } else {
                            tracing::warn!("egui 纹理 {id:?} 的分块更新缺少纹理，已忽略");
                        }
                    }
                }
            }
        }

        for id in &delta.free {
            self.textures.remove(id);
        }
        delta.clear();
    }

    /// 创建整张纹理并写入数据。
    fn create_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        id: TextureId,
        width: u32,
        height: u32,
        pixels: &[u8],
    ) -> TextureEntry {
        let label = format!("lumino_egui_texture_{id:?}");
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(&label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            // 与 egui-wgpu 一致：纹理存 gamma 值，着色器负责 sRGB 目标转换。
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&label),
            layout: &self.texture_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        write_texture(queue, &texture, [0, 0], width, height, pixels);

        TextureEntry {
            texture,
            bind_group,
        }
    }
}

/// 写入一张（子）纹理。
fn write_texture(
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    origin: [u32; 2],
    width: u32,
    height: u32,
    pixels: &[u8],
) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: origin[0],
                y: origin[1],
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * width),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
}
