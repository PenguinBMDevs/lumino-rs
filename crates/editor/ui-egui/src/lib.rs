//! egui UI 层草稿 / 生产目标 crate。
//!
//! 本 crate 的目标是把 旧版 UI 的视觉与布局迁移到 egui，同时保持
//! `lumino-gfx` 的音符场渲染完全不变。当前阶段只实现 4 个固定场景的
//! 草稿骨架，用于与 `tests/ui-baseline/legacy/` 做视觉对比。
//!
//! 设计约束：
//! - 不依赖 `lumino-ui` / `lumino-ui-core`（它们会把 旧版 / wgpu27 带进来）；
//! - 真实 SVG 图标、真实尺寸、真实主题色；
//! - 复杂排版优先使用 `egui_extras` 的 `TableBuilder` / `StripBuilder`，
//!   表单优先使用 `egui::Grid`，避免手写绝对坐标堆叠。

pub mod area_palette;
pub mod audio_export;
pub mod autosave_recover;
pub mod batch_edit;
pub mod bend_buttons;
pub mod brush_settings;
#[cfg(feature = "capture")]
pub mod capture;
pub mod chrome;
pub mod cloud_browser;
pub mod cloud_connect;
pub mod collaboration;
pub mod curve_preview;
pub mod custom_precision;
pub mod dialog;
pub mod draw_settings;
pub mod editor_action_buttons;
pub mod editor_overlay;
pub mod export_progress;
pub mod fonts;
pub mod help;
pub mod history;
pub mod icons;
pub mod image_to_midi;
pub mod layout;
pub mod load_confirm;
pub mod marquee;
pub mod mask_panel;
pub mod material_delete;
pub mod memory_monitor;
pub mod mixer;
pub mod overlay_window;
pub mod piano_roll_context_menu;
pub mod project_settings;
pub mod recover_track;
pub mod remote_cursors;
pub mod renderer_panel;
pub mod scene;
pub mod scrollbar;
pub mod settings;
pub mod shape_canvas;
pub mod shape_drawer;
pub mod shape_edit;
pub mod shape_list;
pub mod shape_preview;
pub mod speed_change;
pub mod text_tool;
pub mod theme;
pub mod velocity_panel;
pub mod video_clip_timeline;
pub mod video_export;
pub mod video_export_overlay;
pub mod video_preview;
pub mod video_timeline_scrollbar;
pub mod widgets;

#[cfg(feature = "capture")]
pub use capture::CapturedImage;
pub use scene::{DPI_SCALES, SceneId, baseline_dir, baseline_file_name};
