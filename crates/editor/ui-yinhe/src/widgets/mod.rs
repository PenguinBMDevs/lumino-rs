//! 小部件 — yinhe `widgets/*` 的 iced 迁移桩
//!
//! 原 `yinhe-egui/src/widgets` 下 17 文件（`scrollbar 1088`、`time_ruler 608`、
//! `grid_lines 512` 等）在 lumino 侧改以 iced 实现：
//! - `scrollbar` 用 `iced_widget::canvas::Program`（thumb 拖拽/边缘缩放）
//! - `time_ruler` 用 `canvas` 矢量层（小节/拍/十六分音符标签）
//! - 其余（`color_picker`、`quantize_popup`、`split_handle` 等）以
//!   `container + column + button` 组合，图标走 SVG，字体走 `Theme`

pub mod checkbox;
pub mod color_picker;
pub mod grid_lines;
pub mod hint;
pub mod hover;
pub mod icon_text;
pub mod menu;
pub mod numeric_input;
pub mod quantize_button;
pub mod quantize_popup;
pub mod reorder;
pub mod scrollbar;
pub mod selection_actions;
pub mod split_handle;
pub mod time_ruler;
pub mod tools_panel;
