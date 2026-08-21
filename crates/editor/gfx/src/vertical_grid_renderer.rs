//! 纵向卷帘网格渲染器 — 横向 `GridRenderer` 的转置版
//!
//! 复用 `GridCameraUniform` / `GridPrepareParams` 的同一 uniform 布局与参数，
//! 仅 Fragment Shader 转置：X=key*zoom_y, Y=tick*zoom_x，键盘在底部、标尺在顶部。
//! 八度分割线（C 音）在纵向更醒目（2px/0.95 alpha），保证 128/256 键范围肉眼可辨。

use crate::gpu_resource_tracker::TrackedBuffer;
use crate::grid_renderer::{GridCameraUniform, GridPrepareParams};

/// 纵向网格渲染器
pub struct VerticalGridRenderer {
    pipeline: wgpu::RenderPipeline,
    camera_buffer: TrackedBuffer,
    bind_group: wgpu::BindGroup,
    cached_uniform: Option<GridCameraUniform>,
}

impl VerticalGridRenderer {
    const SHADER_SRC: &'static str = include_str!("shaders/infinite_grid_vertical.wgsl");

    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, true)
    }

    pub fn new_without_depth(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, false)
    }

    fn new_with_depth(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        needs_depth: bool,
    ) -> Self {
        let shader = crate::shader::create_shader_module(
            device,
            "infinite_grid_vertical_shader",
            Self::SHADER_SRC,
        );

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("infinite_grid_vertical_bind_group_layout"),
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
        });

        let pipeline = crate::pipeline::RenderPipelineBuilder::new(
            device,
            "infinite_grid_vertical_pipeline",
            &shader,
        )
        .bind_group(&bind_group_layout)
        .triangle_strip()
        .alpha_blended_target(format)
        .depth_stencil(crate::constants::rendering::depth_stencil_state_read_only_for(needs_depth))
        .build();

        let camera_buffer = TrackedBuffer::new_init(
            device,
            &wgpu::util::BufferInitDescriptor {
                label: Some("infinite_grid_vertical_camera_uniform"),
                contents: bytemuck::cast_slice(&[GridCameraUniform::builder().build()]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            },
        );

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("infinite_grid_vertical_bind_group"),
            layout: &bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.inner().as_entire_binding(),
            }],
        });

        Self {
            pipeline,
            camera_buffer,
            bind_group,
            cached_uniform: None,
        }
    }

    pub fn prepare(&mut self, queue: &wgpu::Queue, params: &GridPrepareParams) {
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

    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, _instance_count: u32) {
        puffin::profile_function!();
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, &self.bind_group, &[]);
        render_pass.draw(0..4, 0..1);
    }
}
