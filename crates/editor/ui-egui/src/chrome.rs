//! 主窗口外壳：标题栏 + 工具栏 + 左右侧栏 + 状态栏 + 钢琴卷帘骨架。

pub mod piano_roll;
pub mod sidebar;
pub mod statusbar;
pub mod titlebar;
pub mod toolbar;

use egui::{Frame, Margin, Panel, Stroke};

use crate::icons::IconCache;
use crate::layout;
use crate::theme;

/// 音轨列表草稿数据（与生产 `Sidebar.tracks` 的最小子集）。
#[derive(Debug, Clone)]
pub struct TrackRow {
    /// 轨道标签（Conductor / A01）。
    pub label: String,
    /// 轨道名称。
    pub name: String,
    /// 是否指挥轨。
    pub conductor: bool,
    /// 是否选中。
    pub selected: bool,
    /// 轨道颜色（None = 默认）。
    pub color: Option<egui::Color32>,
}

/// 主窗口草稿状态。
pub struct ChromeState {
    /// 图标缓存。
    pub icons: IconCache,
    /// 音轨列表。
    pub tracks: Vec<TrackRow>,
    /// 钢琴卷帘视图。
    pub piano_roll: piano_roll::PianoRollView,
}

impl Default for ChromeState {
    fn default() -> Self {
        Self {
            icons: IconCache::default(),
            tracks: vec![
                TrackRow {
                    label: "Conductor".to_owned(),
                    name: "Conductor".to_owned(),
                    conductor: true,
                    selected: true,
                    color: None,
                },
                TrackRow {
                    label: "A01".to_owned(),
                    name: "Setup".to_owned(),
                    conductor: false,
                    selected: false,
                    color: None,
                },
            ],
            piano_roll: piano_roll::PianoRollView::default(),
        }
    }
}

impl ChromeState {
    /// 钢琴卷帘骨架场景：缩小时间轴并滚动到第 9 小节。
    pub fn piano_roll_skeleton() -> Self {
        Self {
            piano_roll: piano_roll::PianoRollView {
                first_measure: 9,
                pixels_per_measure: 384.0,
            },
            ..Self::default()
        }
    }
}

/// 渲染主窗口外壳。
pub fn show(ui: &mut egui::Ui, state: &mut ChromeState) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());
    ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

    Panel::top("lumino-titlebar")
        .exact_size(layout::TITLEBAR_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(theme::TITLEBAR_BG))
        .show(ui, |ui| titlebar::show(ui, state));

    Panel::bottom("lumino-statusbar")
        .exact_size(layout::STATUSBAR_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(theme::TOOLBAR_BG))
        .show(ui, statusbar::show);

    Panel::left("lumino-sidebar")
        .exact_size(layout::SIDEBAR_W)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(theme::PANEL_BG))
        .show(ui, |ui| sidebar::show(ui, state));

    // 生产 iced 结构：`column![toolbar, row![editor, right_bar]]`——
    // 工具栏横跨「编辑区 + 右侧栏」，右侧栏从工具栏下沿开始。
    Panel::top("lumino-toolbar")
        .exact_size(layout::TOOLBAR_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(theme::PANEL_BG))
        .show(ui, |ui| toolbar::show(ui, state));

    Panel::right("lumino-right-sidebar")
        .exact_size(layout::RIGHT_SIDEBAR_W)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(theme::ROUTE_BAR_BG))
        .show(ui, |ui| sidebar::show_right(ui, state));

    egui::CentralPanel::default()
        .frame(panel_frame(theme::BASE_BG))
        .show(ui, |ui| piano_roll::show(ui, state));
}

pub(crate) fn panel_frame(fill: egui::Color32) -> Frame {
    Frame::default()
        .fill(fill)
        .inner_margin(Margin::ZERO)
        .outer_margin(Margin::ZERO)
        .stroke(Stroke::new(0.0, egui::Color32::TRANSPARENT))
}
