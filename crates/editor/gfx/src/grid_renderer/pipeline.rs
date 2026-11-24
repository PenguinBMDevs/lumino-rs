//! 网格渲染器 — 渲染管线与 Bind Group 创建

use super::*;

impl GridRenderer {
    /// 顶点着色器代码
    const SHADER_SRC: &'static str = include_str!("../shaders/infinite_grid.wgsl");

    /// 创建新的网格渲染器（默认带 depth attachment）
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, true)
    }

    /// 创建不带 depth attachment 的网格渲染器（用于视频导出等纯 2D 路径）
    pub fn new_without_depth(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, false)
    }

    fn new_with_depth(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        needs_depth: bool,
    ) -> Self {
        let shader =
            crate::shader::create_shader_module(device, "infinite_grid_shader", Self::SHADER_SRC);

        // 创建 bind group layout
        let bind_group_layout =
            GridCameraBinding::create_layout(device, "infinite_grid_bind_group_layout");

        // 创建渲染管线，按 needs_depth 决定是否携带 depth-stencil 状态
        let pipeline =
            crate::pipeline::RenderPipelineBuilder::new(device, "infinite_grid_pipeline", &shader)
                .bind_group(&bind_group_layout)
                // 放弃 CPU 传递顶点（无 vertex buffer）
                .triangle_strip()
                .alpha_blended_target(format)
                .depth_stencil(
                    crate::constants::rendering::depth_stencil_state_read_only_for(needs_depth),
                )
                .build();

        let binding = GridCameraBinding::new(
            device,
            &bind_group_layout,
            "infinite_grid_camera_uniform",
            "infinite_grid_bind_group",
        );

        Self { pipeline, binding }
    }
}
