//! 网格渲染器 — 渲染管线与 Bind Group 创建

use super::*;

impl GridRenderer {
    /// 创建新的网格渲染器（默认带 depth attachment，横向）
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, true, GridOrientation::Horizontal)
    }

    /// 创建不带 depth attachment 的网格渲染器（用于视频导出等纯 2D 路径，横向）
    pub fn new_without_depth(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, false, GridOrientation::Horizontal)
    }

    /// 创建纵向网格渲染器（转置版：键盘在底部、标尺在顶部，带 depth attachment）
    pub fn new_vertical(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, true, GridOrientation::Vertical)
    }

    /// 创建不带 depth attachment 的纵向网格渲染器
    pub fn new_vertical_without_depth(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self::new_with_depth(device, format, false, GridOrientation::Vertical)
    }

    fn new_with_depth(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        needs_depth: bool,
        orientation: GridOrientation,
    ) -> Self {
        let prefix = orientation.label_prefix();
        let shader = crate::shader::create_shader_module(
            device,
            &format!("{prefix}_shader"),
            orientation.shader_src(),
        );

        // 创建 bind group layout
        let bind_group_layout =
            GridCameraBinding::create_layout(device, &format!("{prefix}_bind_group_layout"));

        // 创建渲染管线，按 needs_depth 决定是否携带 depth-stencil 状态
        let pipeline = crate::pipeline::RenderPipelineBuilder::new(
            device,
            &format!("{prefix}_pipeline"),
            &shader,
        )
        .bind_group(&bind_group_layout)
        // 放弃 CPU 传递顶点（无 vertex buffer）
        .triangle_strip()
        .alpha_blended_target(format)
        .depth_stencil(crate::constants::rendering::depth_stencil_state_read_only_for(needs_depth))
        .build();

        let binding = GridCameraBinding::new(
            device,
            &bind_group_layout,
            &format!("{prefix}_camera_uniform"),
            &format!("{prefix}_bind_group"),
        );

        Self { pipeline, binding }
    }
}
