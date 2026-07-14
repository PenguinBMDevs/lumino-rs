//! 工具栏事件处理器
//!
//! 通过 crate 模块架构拆分为三个子模块：
//! - `handler` — ToolbarHandler 结构体定义与主入口
//! - `playback` — 播放控制相关方法（play/stop/record 等）
//! - `tools` — 工具选择与音符编辑操作

mod handler;
mod playback;
mod tools;

pub use handler::ToolbarHandler;
