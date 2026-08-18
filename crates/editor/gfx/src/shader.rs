//! Shader 模块创建统一入口
//!
//! 消除各 renderer 中重复的 `create_shader_module` 样板（见
//! docs/Lumino重构方案/文档三 §1.4 —— 原 11 处分散创建收敛到 1 个函数）。
//!
//! 说明：未引入文档草案中的 HashMap ShaderCache，因为每个 shader 仅在各
//! renderer 初始化时加载一次、无重复编译场景，缓存只会引入无用的全局状态
//! 与 dead_code。统一入口函数已消除样板重复。

/// 创建 WGSL shader 模块。
pub fn create_shader_module(
    device: &wgpu::Device,
    label: &str,
    source: &str,
) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}
