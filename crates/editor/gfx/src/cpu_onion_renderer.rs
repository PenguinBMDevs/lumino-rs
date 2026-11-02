//! CPU 洋葱皮透明叠加层（B 方案 Step 2 / Step 5b：脏矩形局部上传）。
//!
//! CPU 光栅（`lumino-cpu-onion`）产出透明背景的 RGBA8 sRGB 帧，本模块把它作为
//! **屏幕空间四边形**贴到主渲染目标上，绘制顺序位于「背景网格之后、GPU 洋葱皮
//! 与当前轨音符之前」（见 `render_thread::render_loop::render_pass`）。
//!
//! 职责边界：
//! - 只做「CPU 像素 → 纹理上传 → 四边形绘制」，不做 DPI 缩放
//!   （`x`/`y`/`width`/`height` 均为物理像素，由调用方换算）；
//! - 不做 CPU 光栅；**脏矩形局部上传**：帧带合法 `dirty_rect` 时只重写该子矩形
//!   （帧内像素坐标，与四边形屏幕位置 `x`/`y` 无关），其余像素保留上一版；
//!   脏区不可信 / 尺寸变化 / 首帧时保守全量；
//! - 默认零行为变化：`RenderParams::cpu_onion_frame` 为 `None` 时渲染线程
//!   完全不会调用本模块。
//!
//! # 模块结构
//!
//! - [`frame`] — [`CpuOnionFrame`] 帧数据、脏区与尺寸校验
//! - [`rect`] — [`CpuOnionUploadRect`] 帧内像素矩形（脏区几何）
//! - [`upload`] — 上传决策状态机（纯逻辑，可脱离 GPU 单测）
//! - [`pipeline`] — 管线 / uniform / bind group layout
//! - [`texture`] — 纹理资源、全量与局部上传
//! - [`renderer`] — 组合入口 [`CpuOnionRenderer`] 的 `prepare` / `draw`

mod frame;
mod pipeline;
mod rect;
mod renderer;
mod texture;
mod upload;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_partial;
#[cfg(test)]
mod tests_support;

pub use frame::CpuOnionFrame;
pub use rect::CpuOnionUploadRect;
pub use renderer::CpuOnionRenderer;
