/// 侧边栏模块 — 路由、面板、音轨列表
mod color_picker;
mod context_menu;
mod core;
mod handling;
mod panel;
mod panel_context_menu;
mod track_reorder;
mod view;

pub mod event;
mod route;

pub use core::{
    GroupId, MIXER_DEFAULT_VOLUME, MIXER_MAX_VOLUME, RESIZE_HANDLE_WIDTH, ROUTES, RollBarButton,
    Route, RouteConfig, Sidebar, Track, gain_to_volume, volume_to_gain,
};
pub use event::Event;
pub use track_reorder::TrackReorderState;

// ── 单测拆分（避免单文件超 400 行）：直接挂载为同级 file 模块，
//    避免 `mod.rs`（被 clippy 禁止）与目录/文件同名碰撞。
// - `panel_tests`：编排模式 / 面板互斥 / 右键菜单 / 颜色选择
// - `roll_bar_tests`：卷帘面板底部按钮（横向/纵向三条杠）互斥与显隐
#[cfg(test)]
mod panel_tests;
#[cfg(test)]
mod roll_bar_tests;
