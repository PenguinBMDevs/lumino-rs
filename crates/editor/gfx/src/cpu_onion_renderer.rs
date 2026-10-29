//! CPU 洋葱皮透明叠加层（B 方案 Step 2）。
//!
//! CPU 光栅（`lumino-cpu-onion`）产出透明背景的 RGBA8 sRGB 帧，本模块把它作为
//! **屏幕空间四边形**贴到主渲染目标上，绘制顺序位于「背景网格之后、GPU 洋葱皮
//! 与当前轨音符之前」（见 `render_thread::render_loop::render_pass`）。
//!
//! 职责边界：
//! - 只做「一帧 CPU 像素 → 一次纹理上传 → 一次四边形绘制」，不做 DPI 缩放
//!   （`x`/`y`/`width`/`height` 均为物理像素，由调用方换算）；
//! - 不做 CPU 光栅、不做局部脏区上传（本步为整帧全量上传）；
//! - 默认零行为变化：`RenderParams::cpu_onion_frame` 为 `None` 时渲染线程
//!   完全不会调用本模块。
//!
//! # 模块结构
//!
//! - [`frame`] — [`CpuOnionFrame`] 帧数据与尺寸校验
//! - [`upload`] — 上传决策状态机（纯逻辑，可脱离 GPU 单测）
//! - [`pipeline`] — 管线 / uniform / bind group layout
//! - [`texture`] — 纹理资源与全量上传
//! - [`renderer`] — 组合入口 [`CpuOnionRenderer`] 的 `prepare` / `draw`

mod frame;
mod pipeline;
mod renderer;
mod texture;
mod upload;

#[cfg(test)]
mod tests;

pub use frame::CpuOnionFrame;
pub use renderer::CpuOnionRenderer;
