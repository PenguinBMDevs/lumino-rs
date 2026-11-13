/// 侧边栏模块 — 路由、面板、音轨列表
///
/// 视图（路由栏 / 音轨列表 / 菜单 / 重命名 / 取色 / 端口·通道 / 拖拽排序）
/// 已全部迁到 egui（`lumino-ui-egui::chrome::sidebar`）；本模块只保留
/// **状态与事件处理**，以及 iced 侧的同宽占位（`view.rs`）。
mod core;
mod handling;
mod metrics;
mod track_reorder;

pub mod event;

pub use core::{
    GroupId, MIXER_DEFAULT_VOLUME, MIXER_MAX_VOLUME, RESIZE_HANDLE_WIDTH, ROUTES, RollBarButton,
    Route, RouteConfig, Sidebar, Track, gain_to_volume, volume_to_gain,
};
pub use event::Event;
pub use track_reorder::{TRACK_ROW_HEIGHT, TRACK_ROW_OFFSET_Y, TrackReorderState};

#[cfg(test)]
mod panel_tests;
#[cfg(test)]
mod roll_bar_tests;
