//! lumino-ui-yinhe — yinhe 副模式的 iced UI 实现
//!
//! P2 阶段仅完成 chrome（标题/传输/模式栏）的 egui→iced 迁移桩，
//! P3 完成 arrange / piano_view / mix 迁移，
//! P5 完成 right_panel（info_panel / event_browser / sf_list / soundfont / project_info）桩。

// 任务要求 `chrome/mod.rs`（见 P2 交付约束），与 workspace `clippy::mod_module_files = deny`
// 冲突；此处显式 allow 以保证 P2 产物可通过 clippy，同时保留任务指定的文件布局。
#![allow(clippy::mod_module_files)]
#![allow(clippy::doc_lazy_continuation)]

pub mod arrange;
pub mod chrome;
pub mod dialogs;
pub mod file;
pub mod piano_view;
pub mod platform;
pub mod right_panel;
pub mod shortcuts;
pub mod state;
pub mod theme;
pub mod widgets;

/// 滚轮增量 → 缩放因子（与 `lumino-ui-editor` / `yinhe` 的 `zoom_factor_from_delta` 一致）
///
/// - `Lines`：每刻度 10 像素为一步 → 1.1×
/// - `Pixels`：每 50 像素为一步 → 1.1×
///
/// `Pixels::y == 0` 且 `Lines::y == 0` 时返回 `None`（无缩放）。
#[must_use]
pub fn zoom_factor_from_delta(delta: &iced_core::mouse::ScrollDelta) -> Option<f32> {
    let dy = match delta {
        iced_core::mouse::ScrollDelta::Lines { y, .. } => *y * 10.0,
        iced_core::mouse::ScrollDelta::Pixels { y, .. } => *y,
    };
    if dy.abs() < f32::EPSILON {
        return None;
    }
    // 每 50px 为一次步进
    let steps = dy / 50.0;
    Some(1.1_f32.powf(steps))
}
