//! 贴图瀑布流 wgpu 渲染器
//!
//! 管理多个整合组贴图的 GPU 纹理，按视口可见性上传/淘汰，
//! 每帧绘制可见贴图。每张贴图覆盖一个 area 矩形（framebuffer 像素）。

mod core_impl;
mod drawing;
mod texture;
mod uniform;

#[cfg(test)]
mod tests;

pub use core_impl::TextureWaterfallRenderer;
pub use uniform::TextureWaterfallUniform;
