//! 走带（Arrange）— yinhe `arrange/` 7 文件的 iced 迁移桩
//!
//! - `track_panel` — 音轨列表 row+scrollable（对齐 `track_panel.rs:1046`）
//! - `view_ui`     — 走带视口 canvas Program（对齐 `view_ui.rs:377` + `view_ui/interaction.rs` / `render.rs`）

pub mod track_panel;
pub mod view_ui;

pub use track_panel::{TrackPanelState, TrackRow};
pub use view_ui::{ArrangeCanvas, ArrangeViewState, ArrangeViewport};
