//! 视频剪辑面板（瀑布流预览 + 时间轴 + 设置）
//!
//! 模仿 nezha `panels.rs` + `piano_view.rs` 的布局与预览模型，
//! 首级面板从空白改为三段式：预览区 | 时间轴 | 设置区。

pub mod layout;
pub mod preview;
pub mod timeline;
pub mod timeline_canvas;
