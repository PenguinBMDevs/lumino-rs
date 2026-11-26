//! 网格摄像机的 Uniform 绑定（buffer + bind group + 变化才上传）。
//!
//! 通用逻辑在 [`UniformBinding`]；本类型只负责 domain 转换与既有 API。

use super::*;
use crate::uniform_binding::UniformBinding;

/// 网格摄像机 Uniform 绑定。
pub(crate) struct GridCameraBinding(UniformBinding<GridCameraUniform>);

impl GridCameraBinding {
    /// 创建 bind group layout（pipeline 按 binding 使用）。
    pub(crate) fn create_layout(device: &wgpu::Device, label: &str) -> wgpu::BindGroupLayout {
        UniformBinding::<GridCameraUniform>::create_layout(
            device,
            label,
            wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
        )
    }

    /// 创建 uniform buffer 与 bind group。
    pub(crate) fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer_label: &str,
        bind_group_label: &str,
    ) -> Self {
        Self(UniformBinding::new(
            device,
            layout,
            GridCameraUniform::default(),
            buffer_label,
            bind_group_label,
        ))
    }

    /// 在 prepare 阶段更新 uniform（仅在数据变化时上传）。
    pub(crate) fn prepare(&mut self, queue: &wgpu::Queue, params: &GridPrepareParams) {
        puffin::profile_function!();
        self.0
            .write(queue, GridCameraUniform::from_prepare_params(params));
    }

    /// bind group 供绘制时绑定。
    pub(crate) fn bind_group(&self) -> &wgpu::BindGroup {
        self.0.bind_group()
    }
}
