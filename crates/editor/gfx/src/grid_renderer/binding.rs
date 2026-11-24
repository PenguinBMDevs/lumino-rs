//! 网格相机 Uniform 绑定：buffer + bind group + 变化检测上传。
//!
//! 横向 / 纵向网格共用同一 uniform 布局与上传策略，只有 label 不同；
//! 本模块把原先两份逐行相同的 `new` 尾部与 `prepare` 收敛为一份。

use super::*;
use crate::gpu_resource_tracker::TrackedBuffer;

/// 相机 Uniform 绑定（buffer + bind group + 缓存）
pub(crate) struct GridCameraBinding {
    camera_buffer: TrackedBuffer,
    bind_group: wgpu::BindGroup,
    cached_uniform: Option<GridCameraUniform>,
}

impl GridCameraBinding {
    /// 创建 bind group layout（pipeline 与 binding 共用）。
    pub(crate) fn create_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some(label),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }

    /// 创建 uniform buffer 与 bind group。
    pub(crate) fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer_label: &str,
        bind_group_label: &str,
    ) -> Self {
        let camera_buffer = TrackedBuffer::new_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some(buffer_label),
                contents: bytemuck::cast_slice(&[GridCameraUniform::builder().build()]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(bind_group_label),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.inner().as_entire_binding(),
            }],
        });

        Self {
            camera_buffer,
            bind_group,
            cached_uniform: None,
        }
    }

    /// 按 prepare 参数构造 uniform，仅在内容变化时上传。
    pub(crate) fn prepare(&mut self, queue: &wgpu::Queue, params: &GridPrepareParams) {
        puffin::profile_function!();
        let viewport = GridCameraUniform::builder()
            .viewport_size(params.viewport_size.0, params.viewport_size.1)
            .camera_pos(params.scroll_x, params.scroll_y)
            .zoom(params.zoom_x, params.zoom_y)
            .margins(params.keyboard_width, params.ruler_height)
            .color_bg(params.color_bg)
            .color_bg_black_key(params.color_bg_black_key)
            .color_bar(params.color_bar)
            .color_beat(params.color_beat)
            .color_half_beat(params.color_half_beat)
            .color_grid(params.color_grid)
            .color_key_line(params.color_key_line)
            .ppq(params.ppq)
            .max_key_index(params.max_key_index)
            .canvas_offset(params.canvas_offset_x, params.canvas_offset_y)
            .canvas_size(params.canvas_size.0, params.canvas_size.1)
            .time_signatures(params.time_signatures.clone())
            .build();

        if self.cached_uniform.as_ref() != Some(&viewport) {
            queue.write_buffer(
                self.camera_buffer.inner(),
                0,
                bytemuck::cast_slice(&[viewport]),
            );
            self.cached_uniform = Some(viewport);
        }
    }

    /// bind group 供绘制时绑定。
    pub(crate) fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}
