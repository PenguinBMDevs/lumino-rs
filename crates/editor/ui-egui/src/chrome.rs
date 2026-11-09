//! 主窗口外壳：标题栏 + 工具栏 + 左右侧栏 + 状态栏 + 钢琴卷帘骨架。

pub mod menu;
mod model;
pub mod overflow;
pub mod piano_roll;
pub mod right_panel;
pub mod sidebar;
pub mod statusbar;
pub mod titlebar;
pub mod tool_panel;
pub mod toolbar;
pub mod toolbar_popup;

pub use model::{
    AudioControlModel, AudioField, AudioPageModel, AudioRowModel, BrushPanelModel, CloudConnModel,
    CloudPageModel, FillDivisionPanelModel, I2mControlModel, I2mConvertModel, I2mField,
    I2mPanelModel, I2mRowModel, MaterialContextLabels, MaterialContextMenuItem,
    MaterialContextMenuModel, MaterialItemModel, MaterialMenuEntryModel, MaterialMenuKind,
    MaterialSectionModel, MaterialsPanelModel, RightSidebarPanelModel, ShapeKind, ShapePanelModel,
    TextParamsPanelModel, ThemeCardModel, ThicknessPanelModel, ToolPanelItemKind,
    ToolPanelItemModel, ToolPanelModel, ToolPanelPanelOwner, ToolPanelSettingsPanel, UiField,
    UiPageModel, UiRow, WATERFALL_TEXTURE_ID, WaterfallPanelModel,
};
pub use model::{
    BatchEditFieldModel, BatchEditModel, BatchEditRowModel, ChromeModel, CompatLine,
    CompatibilityPageModel, CustomPrecisionModel, DialogModel, DialogPalette, DotOptionModel,
    ExportMessageKind, ExportProgressModel, HistoryModel, HistoryRowModel, MenuBarModel, MenuEntry,
    MenuModel, OnionMenuItem, OnionMenuModel, OnionSkinPageModel, OverflowItemModel,
    OverflowMenuModel, ProjectNoteCountModel, ProjectSettingsModel, RightSidebarModel,
    RightSidebarPanelKind, RollBar, SaveConfirmModel, SettingsLabels, SettingsPageModel,
    SettingsPalette, SettingsSnapshot, ShortcutsPageModel, SidebarGroup, SidebarModel,
    SidebarPanel, SidebarTrack, SnapMenuItem, SnapMenuModel, SpeedChangeModel, StatusbarModel,
    TitlebarModel, ToolbarGroupsModel, ToolbarModel,
};
pub use toolbar_popup::ToolbarAction;

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
        .frame(panel_frame(theme::titlebar_bg()))
        .show(ui, |ui| titlebar::show(ui, state));

    Panel::bottom("lumino-statusbar")
        .exact_size(layout::STATUSBAR_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(theme::toolbar_bg()))
        .show(ui, statusbar::show);

    Panel::left("lumino-sidebar")
        .exact_size(layout::SIDEBAR_W)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(theme::panel_bg()))
        .show(ui, |ui| sidebar::show(ui, state));

    // 生产 iced 结构：`column![toolbar, row![editor, right_bar]]`——
    // 工具栏横跨「编辑区 + 右侧栏」，右侧栏从工具栏下沿开始。
    Panel::top("lumino-toolbar")
        .exact_size(layout::TOOLBAR_H)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(theme::panel_bg()))
        .show(ui, |ui| toolbar::show(ui, state));

    Panel::right("lumino-right-sidebar")
        .exact_size(layout::RIGHT_SIDEBAR_W)
        .resizable(false)
        .show_separator_line(false)
        .frame(panel_frame(theme::route_bar_bg()))
        .show(ui, |ui| {
            sidebar::show_right_model(ui, &RightSidebarModel::default(), &mut state.icons)
        });

    egui::CentralPanel::default()
        .frame(panel_frame(theme::base_bg()))
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
    // 本帧菜单占用区先清空：未画菜单时不能残留上一帧的矩形（会吞掉 iced 点击）。
    menu::clear_blocked(ui.ctx());

    // 独立对话框窗口：整窗接管，不绘制任何主窗口 band。
    if let Some(dialog) = &model.dialog {
        match dialog {
            DialogModel::SaveConfirm(save) => dialog::show_model(&mut root, save, icons),
            DialogModel::Settings(values) => {
                // 复用设置场景渲染器：图标缓存由叠加层提供。
                let mut state = crate::settings::SettingsState {
                    icons: std::mem::take(icons),
                    selected: values.selected,
                    eraser: values.eraser.clone(),
                    track_add: values.track_add.clone(),
                    log_retention: values.log_retention.clone(),
                    labels: values.labels.clone(),
                    page: values.page.clone(),
                };
                crate::settings::show_with(&mut root, &mut state, &values.palette);
                *icons = state.icons;
            }
            DialogModel::ProjectSettings(model) => {
                crate::project_settings::show(&mut root, model);
            }
            DialogModel::History(model) => {
                crate::history::show(&mut root, model);
            }
            DialogModel::BatchEdit(model) => {
                crate::batch_edit::show(&mut root, model);
            }
            DialogModel::SpeedChange(model) => {
                crate::speed_change::show(&mut root, model);
            }
            DialogModel::CustomPrecision(model) => {
                crate::custom_precision::show(&mut root, model);
            }
            DialogModel::ExportProgress(model) => {
                crate::export_progress::show(&mut root, model);
            }
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
                theme::titlebar_bg()
            } else {
                theme::route_bar_bg()
            }))
            .show(&mut root, |ui| titlebar::show_model(ui, titlebar, icons));
    }

    // 工具栏 band：从左侧栏右边缘开始，铺满剩余宽度（生产 `row![sidebar,
    // column![toolbar, ...]]`），不覆盖左侧栏。顶部偏移跟随标题栏 band：
    // macOS 原生标题栏模式没有 30px 菜单栏行（iced 主内容直接从工具栏开始）。
    if let Some(toolbar) = &model.toolbar {
        let full = root.max_rect();
        let inset = toolbar.left_inset.max(0.0).min(full.width());
        let top = if model.titlebar.is_some() {
            full.top() + layout::TITLEBAR_H
        } else {
            full.top()
        };
        let band = egui::Rect::from_min_size(
            egui::pos2(full.left() + inset, top),
            egui::Vec2::new(full.width() - inset, layout::TOOLBAR_H),
        );
        if band.width() > 0.0 {
            root.painter().rect_filled(band, 0, theme::panel_bg());
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
            root.painter().rect_filled(band, 0, theme::panel_bg());
            let mut child = root.new_child(
                egui::UiBuilder::new()
                    .max_rect(band)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            sidebar::show_model(&mut child, sidebar, icons);
        }
    }

    // 右侧栏 band：生产布局是 `column![toolbar, row![editor, right_bar]]`，
    // 因此图标列从标题栏 + 工具栏下沿开始、到状态栏上沿结束。面板可见且内容
    // 已由 egui 接管时，band 扩到「面板宽 + 48」，覆盖 iced 的同位置内容。
    if let Some(right) = &model.right_sidebar {
        let full = root.max_rect();
        let top = (full.top() + right.top_inset).max(full.top());
        let bottom = (full.bottom() - layout::STATUSBAR_H).max(top);
        let content = right
            .content
            .as_ref()
            .filter(|_| right.panel_visible && bottom > top);
        let total = if content.is_some() {
            right.panel_width + right.bar_width()
        } else {
            right.bar_width()
        };
        let width = total.max(0.0).min(full.width());
        let band = egui::Rect::from_min_max(
            egui::pos2(full.right() - width, top),
            egui::pos2(full.right(), bottom),
        );
        if band.width() > 0.0 && band.height() > 0.0 {
            if let Some(content) = content {
                // 手柄 6px（生产 `RESIZE_HANDLE_WIDTH`）留在左侧，不进入占用列表：
                // iced 仍负责缩放拖拽，只有内容区改由 egui 处理。
                let handle = egui::Rect::from_min_max(
                    band.min,
                    egui::pos2((band.left() + 6.0).min(band.right()), band.bottom()),
                );
                root.painter().rect_filled(
                    handle,
                    0,
                    if right.resizing {
                        theme::accent_strong()
                    } else {
                        theme::panel_bg()
                    },
                );
                let content_rect = egui::Rect::from_min_max(
                    egui::pos2(handle.right(), band.top()),
                    egui::pos2(band.right() - right.bar_width(), band.bottom()),
                );
                if content_rect.width() > 0.0 {
                    right_panel::set_panel_rect(ui.ctx(), content_rect);
                    right_panel::show(&mut root, content_rect, content, icons);
                }
            } else {
                right_panel::clear_panel_rect(ui.ctx());
            }

            let bar = egui::Rect::from_min_max(
                egui::pos2(band.right() - right.bar_width(), band.top()),
                band.max,
            );
            let mut child = root.new_child(
                egui::UiBuilder::new()
                    .max_rect(bar)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            sidebar::show_right_model(&mut child, right, icons);
        } else {
            right_panel::clear_panel_rect(ui.ctx());
        }
    } else {
        right_panel::clear_panel_rect(ui.ctx());
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
        .frame(panel_frame(theme::toolbar_bg()))
        .show(&mut root, |ui| statusbar::show_model(ui, &model.statusbar));

    // 菜单下拉最后画：不受标题栏 30px 裁剪，且盖在所有 band 之上。
    if let Some(titlebar) = &model.titlebar {
        menu::show_overlay(&mut root, titlebar);
    }
    // 工具栏弹层同样画在 band 之外（向下展开会盖住卷帘区）。
    if let Some(toolbar) = &model.toolbar {
        toolbar_popup::show_overlay(&mut root, toolbar);
        overflow::show_overlay(&mut root, toolbar, icons);
    }
    // 悬浮绘制工具条最后画：它的锚定面板要盖在卷帘与工具栏弹层之上。
    if let Some(tool_panel) = &model.tool_panel {
        tool_panel::show_overlay(&mut root, tool_panel, icons);
    } else {
        tool_panel::clear_triggers(ui.ctx());
    }
}

pub(crate) fn panel_frame(fill: egui::Color32) -> Frame {
    Frame::default()
        .fill(fill)
        .inner_margin(Margin::ZERO)
        .outer_margin(Margin::ZERO)
        .stroke(Stroke::new(0.0, egui::Color32::TRANSPARENT))
}
