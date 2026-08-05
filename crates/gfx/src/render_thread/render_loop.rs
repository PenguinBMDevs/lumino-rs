pub mod commands;
pub mod prepare;
pub mod render_pass;
pub mod runner;

pub mod textures;

pub use runner::run_render_thread;

#[cfg(test)]
mod tests;

/// 渲染器对象集合（消除 prepare_renderers / execute_render_pass 的参数重复）
///
/// 将 6 个渲染器捆绑为一个结构体，使渲染管线函数签名更清晰。
pub struct Renderers {
    pub grid: crate::GridRenderer,
    pub note: crate::NoteRenderer,
    /// 洋葱皮渲染器（不透明背景层，在主音轨之前绘制）
    pub onion_skin: crate::NoteRenderer,
    pub ruler: crate::RulerRenderer,
    pub arrangement: crate::ArrangementRenderer,
    pub cc_bar: crate::CcBarRenderer,
}

impl Renderers {
    /// 创建默认渲染器（带 depth attachment，用于普通 UI 预览）。
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        Self {
            grid: crate::GridRenderer::new(device, format),
            note: crate::NoteRenderer::new(device, queue, format),
            onion_skin: crate::NoteRenderer::new_onion_skin(device, queue, format),
            ruler: crate::RulerRenderer::new(device, format),
            arrangement: crate::ArrangementRenderer::new(device, format),
            cc_bar: crate::CcBarRenderer::new(device, format),
        }
    }

    /// 创建视频导出专用渲染器（无 depth attachment，与无 depth 的 RenderPass 兼容）。
    pub fn new_for_video_export(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self {
        Self {
            grid: crate::GridRenderer::new_without_depth(device, format),
            note: crate::NoteRenderer::new_without_depth(device, queue, format),
            onion_skin: crate::NoteRenderer::new_onion_skin(device, queue, format),
            ruler: crate::RulerRenderer::new_without_depth(device, format),
            arrangement: crate::ArrangementRenderer::new_without_depth(device, format),
            cc_bar: crate::CcBarRenderer::new_without_depth(device, format),
        }
    }
}
