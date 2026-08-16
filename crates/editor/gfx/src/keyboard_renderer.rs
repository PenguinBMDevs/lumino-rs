//! 钢琴键盘渲染器 - 使用 wgpu 实例化渲染高效绘制键盘
//!
//! 替代 iced Canvas 绘制，解决黑乐谱编辑时的性能瓶颈

// 子模块定义
pub mod renderer;
pub mod types;

// 公开导出
pub use renderer::KeyboardRenderer;
pub use types::{KeyInstance, KeyboardViewportUniform};
