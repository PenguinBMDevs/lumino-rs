//! 右侧栏模块 — 工具面板、扩展功能区
//!
//! 提供右侧工具栏，用于放置图片转MIDI等扩展功能按钮。

pub mod convert;
mod core;
pub mod material;
mod materials_view;
mod resize;
pub mod view;

pub use convert::{ConvertResult, run_conversion};
pub use core::{
    DEFAULT_PANEL_WIDTH as RIGHT_SIDEBAR_DEFAULT_WIDTH, MAX_PANEL_WIDTH as RIGHT_SIDEBAR_MAX_WIDTH,
    MIN_PANEL_WIDTH as RIGHT_SIDEBAR_MIN_WIDTH, PALETTE_ALGORITHMS,
    RESIZE_HANDLE_WIDTH as RIGHT_SIDEBAR_RESIZE_HANDLE_WIDTH, RightSidebar, RightSidebarPanel,
};
pub use material::{
    MaterialEntry, MaterialLibrary, MaterialSource, copy_material_to_user_dir,
    project_to_material_preview, scan_materials, user_materials_dir,
};
