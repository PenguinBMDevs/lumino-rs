//! 分离渲染线程模式 — UI 线程只负责状态更新和参数生成，WGPU 渲染在独立线程中。
//!
//! 子模块：
//! - `core`: 核心入口点（render_with_separate_thread, redraw_separate_thread, validate_render_thread_ready）
//! - `runner`: 渲染数据收集与参数构建

mod core;
mod runner;
