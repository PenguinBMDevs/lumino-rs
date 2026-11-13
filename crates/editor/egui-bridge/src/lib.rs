//! egui → wgpu 27 最小绘制桥。
//!
//! 主程序的 渲染引擎 与 `lumino-gfx` 共用同一个 `wgpu::Device/Queue`，
//! wgpu 版本被硬绑定在 27。官方 `egui-wgpu 0.37` 依赖 wgpu 30，无法直接接入，
//! 因此本 crate 只实现渲染所需的最小集合：
//!
//! - 字体图集 / 用户纹理上传（`TexturesDelta`）；
//! - `ClippedPrimitive` 的顶点 / 索引缓冲、scissor、预乘 alpha 混合；
//! - 与 `egui-wgpu` 一致的 gamma / sRGB 双路径着色器；
//! - 与 `RendererOptions::PREDICTABLE` 一致的确定性双线性采样。
//!
//! 不实现：`PaintCallback`、多视口、截图、mipmap / 纹理 wrap 模式切换。
//! 迁移到 wgpu 30 后直接替换为官方 `egui-wgpu`。

mod painter;
mod shader;

pub use painter::{Painter, Screen};
