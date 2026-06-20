//! 洋葱皮概览贴图渲染器
//!
//! 在后台线程中将 MIDI 音符渲染为一张 RGBA8_UNORM 贴图，
//! 用于在卷帘区域显示全曲彩色概览。
//!
//! 贴图规格：
//! - 格式：`RGBA8_UNORM`
//! - 宽度：固定 **4096** 像素
//! - 高度：**128**（128key 模式）或 **256**（256key 模式）
//! - 显存占用：2MB 或 4MB，固定不变

mod generate;
mod lifecycle;
mod renderer;
mod types;
mod uniform;

pub use renderer::OnionSkinRenderer;
pub use types::{GenerateProgress, KeyMode, OnionSkinNote, ViewportParams};
