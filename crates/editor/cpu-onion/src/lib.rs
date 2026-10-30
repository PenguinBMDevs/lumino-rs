//! `lumino-cpu-onion` — 纯 CPU 洋葱皮渲染核心（Exact 为 onion-only，总览 LOD 为全曲密度）。
//!
//! 从 `examples/cpu-onionskin` 抽取的**洋葱皮专用**子集：
//! - 只画「非当前轨 + 非静音轨」的音符（主轨由 GPU 渲染器负责，本 crate 不画）；
//! - 输出透明背景的 sRGB RGBA8 帧（背景 alpha = 0，音符 alpha = 255）；
//! - 与 GPU `onion_note.wgsl` 的外观规则一致：调色板色、边框 `×0.4`
//!   （线性空间）、每侧 2px、重叠 first-wins、无抗锯齿；
//! - 保留 SIMD（AVX2 运行时检测 + 标量兜底）与 tokio 多 band 并行；
//! - 极限缩放（可见音符估算超阈值）自动切 [`density`] 总览密度 LOD：蓝色热力图、
//!   透明背景；该档统计全部音轨（与示例总览一致），当前轨仍由 GPU 精确绘制；
//! - 不含 winit / softbuffer / present / screenshot / bench CLI，
//!   也不绘制键盘栏、标尺、网格、背景与主轨。
//!
//! # 坐标口径
//!
//! 本 crate 只负责 canvas 区域，没有 `keyboard_width` / `ruler_height`：
//! `x = tick * zoom_x - scroll_x`，
//! `y = (key_count - 1 - key) * zoom_y - scroll_y`。
//! 调用方把 [`CpuFrame`] 贴到主工程 canvas 区域即可；DPI 缩放由调用方在
//! 相机尺寸/缩放下发前完成。
//!
//! # 滚动增量
//!
//! [`CpuOnionRenderer::shift_frame`] 按整数像素平移已有帧，
//! [`CpuOnionRenderer::render_strip`] 只重画新露出的窄条。小数滚动必须先把
//! 相机 `scroll_x` 对齐到**帧实际滚动值**（整数累计）再渲染窄条，
//! 否则窄条与主区域会产生亚像素错位。
//!
//! # 线程安全
//!
//! [`CpuOnionRenderer`] 内部持有 tokio 多线程 runtime 与 band 缓冲区槽位，
//! 应在单一线程（通常是 UI/渲染线程）独占使用；音符数据通过
//! `Arc<MidiDocument>` 与编辑层共享，渲染期间不做任何音符复制。

pub mod camera;
pub mod color;
pub mod density;
pub mod frame;
pub mod index;
pub mod renderer;

mod band;
mod buffer;
mod compose;
mod notes;
mod simd;
mod slot;

pub use camera::CanvasCamera;
pub use density::{DensityPyramid, OVERVIEW_NOTE_THRESHOLD, overview_note_estimate, uses_overview};
pub use frame::CpuFrame;
pub use index::{LOOKBACK_TICKS, TrackIndex};
pub use renderer::{CpuOnionRect, CpuOnionRenderer};
