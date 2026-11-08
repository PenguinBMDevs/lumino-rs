//! egui UI 层草稿 / 生产目标 crate。
//!
//! 本 crate 的目标是把 iced UI 的视觉与布局迁移到 egui，同时保持
//! `lumino-gfx` 的音符场渲染完全不变。当前阶段只实现 4 个固定场景的
//! 草稿骨架，用于与 `tests/ui-baseline/iced/` 做视觉对比。
//!
//! 设计约束：
//! - 不依赖 `lumino-ui` / `lumino-ui-core`（它们会把 iced / wgpu27 带进来）；
//! - 真实 SVG 图标、真实尺寸、真实主题色；
//! - 复杂排版优先使用 `egui_extras` 的 `TableBuilder` / `StripBuilder`，
//!   表单优先使用 `egui::Grid`，避免手写绝对坐标堆叠。

pub mod batch_edit;
#[cfg(feature = "capture")]
pub mod capture;
pub mod chrome;
pub mod dialog;
pub mod fonts;
pub mod history;
pub mod icons;
pub mod layout;
pub mod project_settings;
pub mod scene;
pub mod settings;
pub mod theme;
pub mod widgets;

#[cfg(feature = "capture")]
pub use capture::CapturedImage;
pub use scene::{DPI_SCALES, SceneId, baseline_dir, baseline_file_name};
