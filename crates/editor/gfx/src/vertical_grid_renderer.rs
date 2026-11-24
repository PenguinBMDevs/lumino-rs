//! 纵向卷帘网格渲染器 — 横向 `GridRenderer` 的转置版
//!
//! 复用 `GridCameraUniform` / `GridPrepareParams` 的同一 uniform 布局与参数，
//! 仅 Fragment Shader 转置：X=key*zoom_y, Y=tick*zoom_x，键盘在底部、标尺在顶部。
//! 八度分割线（C 音）在纵向更醒目（2px/0.95 alpha），保证 128/256 键范围肉眼可辨。

use crate::grid_renderer::{GridCameraBinding, GridPrepareParams};

/// 纵向网格渲染器
pub struct VerticalGridRenderer {
    pipeline: wgpu::RenderPipeline,
    binding: GridCameraBinding,
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

        let bind_group_layout =
            GridCameraBinding::create_layout(device, "infinite_grid_vertical_bind_group_layout");

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

        let binding = GridCameraBinding::new(
            device,
            &bind_group_layout,
            "infinite_grid_vertical_camera_uniform",
            "infinite_grid_vertical_bind_group",
        );

        Self { pipeline, binding }
    }

    pub fn prepare(&mut self, queue: &wgpu::Queue, params: &GridPrepareParams) {
        self.binding.prepare(queue, params);
    }

    pub fn draw<'r>(&'r self, render_pass: &mut wgpu::RenderPass<'r>, _instance_count: u32) {
        puffin::profile_function!();
        render_pass.set_pipeline(&self.pipeline);
        render_pass.set_bind_group(0, self.binding.bind_group(), &[]);
        render_pass.draw(0..4, 0..1);
    }
}
