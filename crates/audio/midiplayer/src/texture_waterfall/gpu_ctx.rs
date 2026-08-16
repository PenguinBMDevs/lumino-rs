//! 贴图瀑布流 GPU 基础设施上下文
//!
//! 从宿主渲染线程（如 gfx 的 RenderContext）解耦，仅保留贴图瀑布流
//! 渲染所需的三个字段，使本模块不依赖任何宿主渲染结构。

/// 贴图瀑布流所需的 GPU 基础设施引用
#[derive(Clone, Copy)]
pub struct WaterfallGpuCtx<'a> {
    /// wgpu 设备
    pub device: &'a wgpu::Device,
    /// wgpu 队列
    pub queue: &'a wgpu::Queue,
    /// 渲染目标纹理格式
    pub texture_format: wgpu::TextureFormat,
}
