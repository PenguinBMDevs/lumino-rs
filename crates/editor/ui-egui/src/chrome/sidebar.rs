//! 左侧栏：路由栏（48）+ 音轨面板（`panel_width`）。
//!
//! 路由栏条目与生产 `ROUTES` 一一对应：
//! - 组父按钮常驻，激活时点亮 `parent_color`；
//! - 子按钮仅在所属分组激活时出现，激活时点亮 `child_color`；
//! - 底部依次为横向 / 纵向卷帘按钮（`roll_bar_visible` 时）与混音台入口。
//!
//! 结构：`route_bar`（路由栏绘制与动作）、`rail`（右侧栏图标列）、`track_list` /
//! `menu` / `picker` / `panel_action`（面板交互）。对外仍从本模块根导出。

mod menu;
mod panel_action;
mod picker;
mod rail;
mod route_bar;
mod track_list;

use egui::{Rect, Vec2};

use super::{ChromeState, SidebarModel, SidebarPanel};
use crate::theme;

pub use panel_action::{
    RenameBlur, SidebarMenuAction, SidebarPanelAction, SidebarRenameBlur, take_menu_action,
    take_panel_action, take_rename_blur,
};
pub use rail::show_right_model;

/// 路由栏宽度（生产 `ROUTE_BAR_WIDTH`）。
pub(crate) const ROUTE_BAR_W: f32 = 48.0;

/// 路由栏条目（与生产 `ROUTES` 语义一一对应，不含 上一代 类型）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SidebarRoute {
    /// 钢琴卷帘组父按钮。
    PianoRollGroup,
    /// 音轨列表子按钮。
    FilePanel,
    /// 自动化面板子按钮。
    AutomationPanel,
    /// 工程走带组父按钮。
    ProjectGroup,
    /// 播放器（瀑布流）组父按钮。
    WaterfallGroup,
    /// 渲染组父按钮。
    RendererGroup,
    /// 视频导出子按钮。
    VideoExport,
    /// 音频导出子按钮。
    AudioExport,
    /// 底部：横向卷帘方向。
    RollBarHorizontal,
    /// 底部：纵向卷帘方向。
    RollBarVertical,
    /// 底部：混音台开关。
    MixerToggle,
}

/// 路由栏动作通道。
const ROUTE_ACTION_ID: &str = "lumino.sidebar.route-action";

/// 记录本帧路由栏点击（一帧一条）。
pub fn push_route_action(ctx: &egui::Context, route: SidebarRoute) {
    if ctx
        .data(|d| d.get_temp::<Option<SidebarRoute>>(egui::Id::unique(ROUTE_ACTION_ID)))
        .flatten()
        .is_some()
    {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(ROUTE_ACTION_ID), Some(route)));
}

/// 取走本帧路由栏点击。
pub fn take_route_action(ctx: &egui::Context) -> Option<SidebarRoute> {
    ctx.data_mut(|d| d.remove_temp::<Option<SidebarRoute>>(egui::Id::unique(ROUTE_ACTION_ID)))
        .flatten()
}

/// 左侧栏：路由栏 + 音轨面板。
pub fn show(ui: &mut egui::Ui, state: &mut ChromeState) {
    show_model(ui, &state.sidebar.clone(), &mut state.icons);
}

/// 按只读模型渲染左侧栏。
pub fn show_model(ui: &mut egui::Ui, model: &SidebarModel, icons: &mut crate::icons::IconCache) {
    let rect = ui.max_rect();
    ui.painter().rect_filled(rect, 0, theme::panel_bg());
    let route = Rect::from_min_size(rect.min, Vec2::new(ROUTE_BAR_W, rect.height()));
    ui.painter().rect_filled(route, 0, theme::route_bar_bg());

    // 路由栏由 egui **独占**：绘制 + 点击 + 动作通道（含底部卷帘方向与混音台）。
    // 因此必须计入占用区，否则同一次点击会同时被 egui 和画布各处理一次
    // （表现为「打开又立刻关闭」，或命中到 上一代 的旧偏移坐标）。
    super::menu::extend_blocked(ui.ctx(), [route]);

    route_bar::route_bar(ui, rect, model, icons);

    if model.panel_visible {
        let panel = Rect::from_min_size(
            egui::pos2(rect.left() + ROUTE_BAR_W, rect.top()),
            Vec2::new((model.panel_width - 6.0).max(0.0), rect.height()),
        );
        match model.panel {
            SidebarPanel::Tracks => track_list::show_model(ui, panel, model, icons),
            SidebarPanel::AddTrackOnly => track_list::show_add_track(ui, panel, model, icons),
            SidebarPanel::Blank => {}
        }
        // 右侧 6px 缩放手柄：该区域会被底层画布纹理盖住，这里由 egui 绘制并接管。
        let handle = Rect::from_min_size(
            egui::pos2(panel.right(), panel.top()),
            Vec2::new(6.0, panel.height()),
        );
        ui.painter().rect_filled(
            handle,
            0,
            if model.resizing {
                theme::accent_strong()
            } else {
                theme::panel_bg()
            },
        );
        let response = ui.interact(
            handle,
            egui::Id::unique("lumino.sidebar.resize"),
            egui::Sense::click_and_drag(),
        );
        let response = response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        let dragging = egui::Id::unique("lumino.sidebar.resizing");
        let started = ui
            .ctx()
            .data(|d| d.get_temp::<bool>(dragging))
            .unwrap_or(false);
        let down = response.is_pointer_button_down_on();
        if down && !started {
            ui.ctx().data_mut(|d| d.insert_temp(dragging, true));
            panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::ResizeStarted);
        } else if !down && started {
            ui.ctx().data_mut(|d| d.insert_temp(dragging, false));
            panel_action::push_panel_action(ui.ctx(), SidebarPanelAction::ResizeEnded);
        }
        crate::chrome::menu::extend_blocked(ui.ctx(), [handle]);
    }
}
