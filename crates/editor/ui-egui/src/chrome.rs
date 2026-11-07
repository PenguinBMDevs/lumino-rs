//! 主窗口外壳：标题栏 + 工具栏 + 左右侧栏 + 状态栏 + 钢琴卷帘骨架。

mod model;
pub mod piano_roll;
pub mod sidebar;
pub mod statusbar;
pub mod titlebar;
pub mod toolbar;

pub use model::{
    ChromeModel, DialogModel, RightSidebarModel, RightSidebarPanelKind, RollBar, SaveConfirmModel,
    SidebarGroup, SidebarModel, SidebarPanel, SidebarTrack, StatusbarModel, TitlebarModel,
    ToolbarModel,
};

use egui::{Frame, Margin, Panel, Stroke};

use crate::dialog;
use crate::icons::IconCache;
use crate::layout;
use crate::theme;

/// 主窗口草稿状态。
#[derive(Default)]
pub struct ChromeState {
    /// 图标缓存。
    pub icons: IconCache,
    /// 左侧栏（路由栏 + 音轨列表）。
    pub sidebar: SidebarModel,
    /// 钢琴卷帘视图。
    pub piano_roll: piano_roll::PianoRollView,
}

impl ChromeState {
    /// 钢琴卷帘骨架场景：缩小时间轴并滚动到第 9 小节。
    pub fn piano_roll_skeleton() -> Self {
        Self {
            piano_roll: piano_roll::PianoRollView {
                // iced PianoRollSkeleton：zoom_x=0.05、滚动到第 9 小节。
                zoom_x: 0.05,
                scroll_x: 8.0 * 384.0,
                ..Default::default()
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
        .show(ui, |ui| {
            sidebar::show_right_model(ui, &RightSidebarModel::default(), &mut state.icons)
        });

    egui::CentralPanel::default()
        .frame(panel_frame(theme::BASE_BG))
        .show(ui, |ui| piano_roll::show(ui, state));
}

/// 只渲染标题栏与状态栏（真实窗口叠加层用）。
///
/// 其余区域不绘制任何东西，保持下层 iced 画面可见；两条 band 用不透明底色
/// 完全覆盖同位置的 iced 版本，避免文字重影。标题栏为 `None`（系统原生
/// 标题栏）时跳过顶部 band。
pub fn show_bands(ui: &mut egui::Ui, model: &ChromeModel, icons: &mut IconCache) {
    theme::apply(ui.ctx());
    crate::fonts::install(ui.ctx());

    // `egui_kittest`（以及部分宿主）给到的 Ui 带 8px 外缩进；Panel 需要铺满视口，
    // 否则 band 会整体内缩、与 iced 原位置错开。生产接线时根 Ui 本就是全屏，
    // 这里重建等价于无操作。
    let full = ui.ctx().viewport_rect();
    let mut root = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(full)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    root.set_clip_rect(full);
    root.spacing_mut().item_spacing = egui::Vec2::ZERO;

    // 独立对话框窗口：整窗接管，不绘制任何主窗口 band。
    if let Some(dialog) = &model.dialog {
        match dialog {
            DialogModel::SaveConfirm(save) => dialog::show_model(&mut root, save, icons),
        }
        return;
    }

    if let Some(titlebar) = &model.titlebar {
        let focused = titlebar.focused;
        Panel::top("lumino-titlebar-band")
            .exact_size(layout::TITLEBAR_H)
            .resizable(false)
            .show_separator_line(false)
            .frame(panel_frame(if focused {
                theme::TITLEBAR_BG
            } else {
                theme::ROUTE_BAR_BG
            }))
            .show(&mut root, |ui| titlebar::show_model(ui, titlebar, icons));
    }

    // 工具栏 band：从左侧栏右边缘开始，铺满剩余宽度（生产 `row![sidebar,
    // column![toolbar, ...]]`），不覆盖左侧栏。
    if let Some(toolbar) = &model.toolbar {
        let full = root.max_rect();
        let inset = toolbar.left_inset.max(0.0).min(full.width());
        let band = egui::Rect::from_min_size(
            egui::pos2(full.left() + inset, full.top() + layout::TITLEBAR_H),
            egui::Vec2::new(full.width() - inset, layout::TOOLBAR_H),
        );
        if band.width() > 0.0 {
            root.painter().rect_filled(band, 0, theme::PANEL_BG);
            let mut child = root.new_child(
                egui::UiBuilder::new()
                    .max_rect(band)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            toolbar::show_model(&mut child, toolbar, icons);
        }
    }

    // 左侧栏 band：生产布局是 `column![titlebar, row![sidebar, content], statusbar]`，
    // 因此左侧栏从标题栏下沿开始、到状态栏上沿结束，宽度 = 48 + panel_width。
    if let Some(sidebar) = &model.sidebar {
        let full = root.max_rect();
        let top = if model.titlebar.is_some() {
            full.top() + layout::TITLEBAR_H
        } else {
            full.top()
        };
        let bottom = full.bottom() - layout::STATUSBAR_H;
        let width = sidebar.total_width().max(0.0).min(full.width());
        let band = egui::Rect::from_min_max(
            egui::pos2(full.left(), top),
            egui::pos2(full.left() + width, bottom.max(top)),
        );
        if band.width() > 0.0 && band.height() > 0.0 {
            root.painter().rect_filled(band, 0, theme::PANEL_BG);
            let mut child = root.new_child(
                egui::UiBuilder::new()
                    .max_rect(band)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            sidebar::show_model(&mut child, sidebar, icons);
        }
    }

    // 右侧栏 band：生产布局是 `column![toolbar, row![editor, right_bar]]`，
    // 因此图标列从标题栏 + 工具栏下沿开始、到状态栏上沿结束，宽 48。
    if let Some(right) = &model.right_sidebar {
        let full = root.max_rect();
        let top = (full.top() + right.top_inset).max(full.top());
        let bottom = (full.bottom() - layout::STATUSBAR_H).max(top);
        let width = right.bar_width().max(0.0).min(full.width());
        let band = egui::Rect::from_min_max(
            egui::pos2(full.right() - width, top),
            egui::pos2(full.right(), bottom),
        );
        if band.width() > 0.0 && band.height() > 0.0 {
            let mut child = root.new_child(
                egui::UiBuilder::new()
                    .max_rect(band)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            sidebar::show_right_model(&mut child, right, icons);
        }
    }

    // 钢琴卷帘骨架 band：只画键盘列 / 标尺行 / 两条滚动条，
    // 中央网格区域保持透明（下层是 `lumino-gfx` 的 wgpu 音符与网格）。
    if let Some(piano) = &model.piano_roll {
        let area = piano.area(root.max_rect());
        if area.width() > 0.0 && area.height() > 0.0 {
            let child = root.new_child(
                egui::UiBuilder::new()
                    .max_rect(area)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            piano_roll::draw_chrome(&child, piano);
        }
    }

    Panel::bottom("lumino-statusbar-band")
        .exact_size(layout::STATUSBAR_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(theme::TOOLBAR_BG))
        .show(&mut root, |ui| statusbar::show_model(ui, &model.statusbar));
}

pub(crate) fn panel_frame(fill: egui::Color32) -> Frame {
    Frame::default()
        .fill(fill)
        .inner_margin(Margin::ZERO)
        .outer_margin(Margin::ZERO)
        .stroke(Stroke::new(0.0, egui::Color32::TRANSPARENT))
}
