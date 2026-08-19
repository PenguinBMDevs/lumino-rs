//! 键盘实例数据类型
//!
//! 早期 wgpu 键盘渲染器（`KeyboardRenderer`）已删除——从未接入渲染线程，
//! 编辑器键盘绘制走 `render_thread` 的 note shader + 视频导出走 CPU 贴图合成。
//! 此处仅保留被 `RenderParams.keyboard_instances` 引用的 `KeyInstance` 类型。

// 子模块定义
pub mod types;

// 公开导出
pub use types::KeyInstance;