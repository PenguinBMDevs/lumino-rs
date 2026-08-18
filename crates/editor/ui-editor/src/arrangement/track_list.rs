//! 工程走带左侧音轨列表 Canvas —— 按 yinhe 风格绘制音轨名称和选中状态
//!
//! 与右侧走带 Canvas 共享 scroll_y，实现同步滚动。
//! 支持长按/拖动音轨行改变音轨顺序：按下注册拖拽候选并同步到 Sidebar
//! 统一计时，移动超阈值或长按超时后激活，释放时发出排序事件。
//! 绘制逻辑在 `track_list/draw.rs`，交互状态在 `track_list/state.rs`，
//! 事件处理在 `track_list/handlers.rs`，Program 实现与测试分别在
//! `track_list/program.rs` 与 `track_list/tests.rs`。

mod draw;
mod handlers;
mod program;
mod state;

#[cfg(test)]
mod tests;

use std::collections::HashSet;

use iced_core::Color;

pub use state::{MuteSoloButton, TrackDragState, TrackListState};

/// 未设置音轨颜色时左侧色块宽度（像素）
pub(crate) const BADGE_WIDTH: f32 = 8.0;
/// 文本左侧边距（像素）
pub(crate) const TEXT_MARGIN: f32 = 6.0;
/// 静音/独奏按钮尺寸（像素）
pub(crate) const BTN_SIZE: f32 = 18.0;
/// 静音/独奏按钮间距（像素）
pub(crate) const BTN_GAP: f32 = 2.0;

/// 工程走带左侧音轨列表 Canvas
pub struct TrackListCanvas {
    /// 音轨列表：(id, name)
    pub tracks: Vec<(usize, String)>,
    /// 每轨显示标签（如 A01），与 tracks 一一对应
    pub track_labels: Vec<String>,
    /// 每轨通道号（用于生成显示标签）
    pub track_channels: Vec<u8>,
    /// 每轨颜色标签
    pub track_colors: Vec<Option<Color>>,
    /// 每轨是否为主控音轨
    pub track_conductors: Vec<bool>,
    /// 每轨静音状态（初始值）
    pub track_muted: Vec<bool>,
    /// 每轨独奏状态（初始值）
    pub track_soloed: Vec<bool>,
    /// 当前选中的音轨 ID（单选兼容）
    pub selected_track: usize,
    /// 当前多选集合（外部传入的初始值）
    pub selected_tracks: HashSet<usize>,
    /// 范围选择锚点
    pub selection_anchor: Option<usize>,
    /// 垂直滚动偏移
    pub scroll_y: f32,
    /// 每轨高度
    pub track_height: f32,
    /// 总高度
    pub total_height: f32,
    /// 垂直缩放倍率（1.0 = 默认高度），Ctrl+滚轮垂直缩放时用于计算新 zoom_y
    pub zoom_y: f32,
    /// Ctrl 键按下状态（窗口级 CtrlKeyChanged 可靠通道，用于 Ctrl+滚轮垂直缩放）
    pub ctrl_pressed: bool,
    /// 外部长按激活的拖拽排序标记（Sidebar 计时，None 表示无拖拽）
    pub drag_active: bool,
}

impl TrackListCanvas {
    pub fn new(
        tracks: Vec<(usize, String)>,
        selected_track: usize,
        scroll_y: f32,
        track_height: f32,
        total_height: f32,
    ) -> Self {
        let count = tracks.len();
        Self {
            tracks,
            track_labels: vec![String::new(); count],
            track_channels: vec![0; count],
            track_colors: vec![None; count],
            track_conductors: vec![false; count],
            track_muted: vec![false; count],
            track_soloed: vec![false; count],
            selected_track,
            selected_tracks: HashSet::new(),
            selection_anchor: None,
            scroll_y,
            track_height,
            total_height,
            zoom_y: 1.0,
            ctrl_pressed: false,
            drag_active: false,
        }
    }

    pub fn with_labels(mut self, labels: Vec<String>) -> Self {
        self.track_labels = labels;
        self
    }

    pub fn with_channels(mut self, channels: Vec<u8>) -> Self {
        self.track_channels = channels;
        self
    }

    pub fn with_colors(mut self, colors: Vec<Option<Color>>) -> Self {
        self.track_colors = colors;
        self
    }

    pub fn with_conductors(mut self, conductors: Vec<bool>) -> Self {
        self.track_conductors = conductors;
        self
    }

    pub fn with_mutes(mut self, muted: Vec<bool>) -> Self {
        self.track_muted = muted;
        self
    }

    pub fn with_solos(mut self, soloed: Vec<bool>) -> Self {
        self.track_soloed = soloed;
        self
    }

    pub fn with_selection(mut self, selected: HashSet<usize>, anchor: Option<usize>) -> Self {
        self.selected_tracks = selected;
        self.selection_anchor = anchor;
        self
    }

    /// 设置外部长按激活的拖拽排序标记（来自 Sidebar 统一计时）
    pub fn with_drag_active(mut self, active: bool) -> Self {
        self.drag_active = active;
        self
    }

    /// 设置垂直缩放倍率（1.0 = 默认高度），与右侧走带视口 zoom_y 保持一致
    pub fn with_zoom_y(mut self, zoom_y: f32) -> Self {
        self.zoom_y = zoom_y;
        self
    }

    /// 设置 Ctrl 键按下状态（窗口级 CtrlKeyChanged 可靠通道）
    pub fn with_ctrl_pressed(mut self, pressed: bool) -> Self {
        self.ctrl_pressed = pressed;
        self
    }
}
