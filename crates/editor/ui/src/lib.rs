//! lumino-ui 编辑器的 UI 入口 crate。
//!
//! 汇聚编辑器 UI 的各个子模块（根视图、标题栏、工具栏、右侧栏等），
//! 并对外重导出 core / editor / settings 等 crate 的核心类型。

#![allow(deprecated)]
/// 常量集合，对外重导出 core crate 的常量。
pub mod constants {
    pub use lumino_ui_core::constants::*;
}
pub use lumino_ui_core::app_mode;
pub use lumino_ui_editor as editor;
pub mod event;
pub mod host;
pub use lumino_ui_core::message;
pub mod mixer;
pub mod playback;
pub(crate) use lumino_ui_core::resources;
pub mod root;
pub use lumino_ui_settings as settings;
/// 右侧栏模块
pub mod right_sidebar;
/// 侧边栏模块（音轨列表、路由栏等）。
pub mod sidebar;
/// 编辑器全局状态（播放、MIDI、根视图状态等）。
pub mod state;
mod statusbar;
#[cfg(test)]
mod test_helpers;
/// 编辑器标题栏模块（菜单、模式切换等）。
pub mod titlebar;
pub mod toast;
pub mod toolbar;
pub mod util;
mod view;
pub mod wgpu_render_thread;
pub(crate) mod widget;
pub use lumino_ui_core::window;

pub use host::{Host, NoteData, TrackNotes, prewarm_dialog_shared_engine};
pub(crate) use lumino_core::storage::config;
pub use root::MemoryBreakdown;
pub use root::Root;
/// 主题集合，对外重导出 core crate 的主题。
pub mod theme {
    pub use lumino_ui_core::theme::*;
}
pub(crate) use lumino_ui_core::{Element, Message, Renderer, Theme};
pub use state::root_state::CollaborationViewState;
pub use wgpu_render_thread::{
    ControlCommand, RenderParams, RenderStats as WgpuRenderStats, WgpuRenderThread,
};
