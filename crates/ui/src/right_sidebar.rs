//! 右侧栏模块 — 工具面板、扩展功能区
//!
//! 提供右侧工具栏，用于放置图片转MIDI等扩展功能按钮。

pub mod convert;
mod core;
pub mod view;

pub use convert::{ConvertResult, run_conversion};
pub use core::{
    DEFAULT_PANEL_WIDTH as RIGHT_SIDEBAR_DEFAULT_WIDTH, MAX_PANEL_WIDTH as RIGHT_SIDEBAR_MAX_WIDTH,
    MIN_PANEL_WIDTH as RIGHT_SIDEBAR_MIN_WIDTH,
    RESIZE_HANDLE_WIDTH as RIGHT_SIDEBAR_RESIZE_HANDLE_WIDTH, RightSidebar,
};
