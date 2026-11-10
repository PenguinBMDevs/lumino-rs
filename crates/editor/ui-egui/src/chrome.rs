//! 主窗口外壳：标题栏 + 工具栏 + 左右侧栏 + 状态栏 + 钢琴卷帘骨架。

pub mod arrangement;
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

pub use arrangement::ArrangementAction;
pub use model::{
    ArrangementLaneModel, ArrangementModel, ArrangementRulerModel, ArrangementTrackListModel,
    ArrangementTrackRow, AudioButtonsMode, AudioButtonsModel, AudioChoice, AudioControlModel,
    AudioExportModel, AudioExportPanelModel, AudioField, AudioFilterModel, AudioOutputModel,
    AudioPageModel, AudioProjectModel, AudioRowModel, AudioSettingsModel, AudioToggle,
    AudioToggleId, BrushPanelModel, CloudConnModel, CloudPageModel, EDITOR_TEXTURE_ID,
    FillDivisionPanelModel, I2mControlModel, I2mConvertModel, I2mField, I2mPanelModel, I2mRowModel,
    MaterialContextLabels, MaterialContextMenuItem, MaterialContextMenuModel, MaterialItemModel,
    MaterialMenuEntryModel, MaterialMenuKind, MaterialSectionModel, MaterialsPanelModel,
    RightSidebarPanelModel, ShapeKind, ShapePanelModel, TextParamsPanelModel, ThemeCardModel,
    ThicknessPanelModel, ToolPanelItemKind, ToolPanelItemModel, ToolPanelModel,
    ToolPanelPanelOwner, ToolPanelSettingsPanel, UiField, UiPageModel, UiRow, WATERFALL_TEXTURE_ID,
    WaterfallPanelModel,
};
pub use model::{
    BatchEditFieldModel, BatchEditModel, BatchEditRowModel, BrushLevelModel, BrushSettingsModel,
    BrushTrackChoice, ChromeModel, CloudBrowserModel, CloudConnectModel, CloudDeviceModel,
    CloudEntryActionKind, CloudEntryLabels, CloudEntryModel, CloudField, CloudFieldModel,
    CloudNoticeModel, CloudProtocol, CloudProtocolOptionModel, CloudSavePanelModel,
    CollaborationConnectView, CollaborationConnectingView, CollaborationInRoomView,
    CollaborationModel, CollaborationRoomActionsView, CollaborationViewModel, CompatLine,
    CompatibilityPageModel, CustomPrecisionModel, DialogModel, DialogPalette, DotOptionModel,
    DrawSettingsModel, ExportMessageKind, ExportProgressModel, HelpModel, HelpSectionModel,
    HelpTopicBodyModel, HelpTopicRowModel, HistoryModel, HistoryRowModel, LoadConfirmModel,
    MemoryMetricModel, MemoryMonitorModel, MemoryRowModel, MenuBarModel, MenuEntry, MenuModel,
    OnionMenuItem, OnionMenuModel, OnionSkinPageModel, OverflowItemModel, OverflowMenuModel,
    ProjectNoteCountModel, ProjectSettingsModel, RecoverTrackModel, RecoverTrackRowModel,
    RecoverTrackSelection, RightSidebarModel, RightSidebarPanelKind, RollBar, SaveConfirmModel,
    SettingsLabels, SettingsPageModel, SettingsPalette, SettingsSnapshot, ShapeActionButton,
    ShapeDrawerModel, ShapePanelAction, ShapeRowModel, ShapeSwatchModel, ShortcutsPageModel,
    SidebarGroup, SidebarModel, SidebarPanel, SidebarRouteIcon, SidebarRouteItem, SidebarTrack,
    SnapMenuItem, SnapMenuModel, SpeedChangeModel, StatusbarModel, TextPairCell, TitlebarModel,
    ToolbarGroupsModel, ToolbarModel, VideoButton, VideoButtonFill, VideoExportAction,
    VideoExportModel, VideoExportOverlayModel, VideoExportPanelModel, VideoOverlayAction,
    VideoOverlayCompleted, VideoOverlayError, VideoOverlayFinalizing, VideoOverlayProgress,
    VideoOverlayView, VideoPreviewModel, VideoRow, VideoSection,
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

    // 底层由 egui 采样 gfx 离屏纹理时，中央面板不能再铺不透明底色（会盖掉画布）。
    let central_frame = if base_texture_active(ui.ctx()) {
        egui::Frame::NONE
    } else {
        panel_frame(theme::base_bg())
    };
    egui::CentralPanel::default()
        .frame(central_frame)
        .show(ui, |ui| piano_roll::show(ui, state));
}

/// 只渲染标题栏与状态栏（真实窗口叠加层用）。
///
/// 其余区域不绘制任何东西，保持下层 iced 画面可见；两条 band 用不透明底色
/// 完全覆盖同位置的 iced 版本，避免文字重影。标题栏为 `None`（系统原生
/// 标题栏）时跳过顶部 band。
/// 记录本帧的走带标尺模型（`None` = 不接管或不在走带路由）。
pub fn set_arrangement(ctx: &egui::Context, model: Option<ArrangementModel>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.arrangement"), model));
}

/// 取走本帧的走带标尺模型。
fn arrangement_model(ctx: &egui::Context) -> Option<ArrangementModel> {
    ctx.data(|d| d.get_temp::<Option<ArrangementModel>>(egui::Id::unique("lumino.arrangement")))
        .flatten()
}

/// 记录本帧的混音台模型（`None` = 面板未打开）。
pub fn set_mixer(
    ctx: &egui::Context,
    model: Option<(crate::mixer::MixerLayout, crate::mixer::MixerModel)>,
) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.mixer"), model));
}

fn mixer_model(
    ctx: &egui::Context,
) -> Option<(crate::mixer::MixerLayout, crate::mixer::MixerModel)> {
    ctx.data(|d| {
        d.get_temp::<Option<(crate::mixer::MixerLayout, crate::mixer::MixerModel)>>(
            egui::Id::unique("lumino.mixer"),
        )
    })
    .flatten()
}

/// 记录本帧的视频剪辑时间轴模型（`None` = 不接管或不在渲染器入口）。
pub fn set_video_timeline(
    ctx: &egui::Context,
    model: Option<(egui::Rect, crate::video_clip_timeline::VideoTimelineModel)>,
) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.video-timeline"), model));
}

/// 取走本帧的视频剪辑时间轴模型。
fn video_timeline(
    ctx: &egui::Context,
) -> Option<(egui::Rect, crate::video_clip_timeline::VideoTimelineModel)> {
    ctx.data(|d| {
        d.get_temp::<Option<(egui::Rect, crate::video_clip_timeline::VideoTimelineModel)>>(
            egui::Id::unique("lumino.video-timeline"),
        )
    })
    .flatten()
}

/// 时间轴是否由 egui 接管。
///
/// **默认开启**：iced 侧的 `TimelineCanvas` 已删除（面板里只剩同尺寸占位），
/// 关闭开关只会得到空白区域；`LUMINO_EGUI_CLIP_TIMELINE=0` 仅用于对照排查。
fn video_timeline_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("LUMINO_EGUI_CLIP_TIMELINE")
            .map(|v| v != "0")
            .unwrap_or(true)
    })
}

/// 记录本帧的底层画布纹理（`None` = 不画底层，走原通道）。
pub fn set_base_texture(ctx: &egui::Context, id: Option<egui::TextureId>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique("lumino.base-texture"), id));
}

/// 本帧是否由 egui 采样底层画布纹理。
pub fn base_texture_active(ctx: &egui::Context) -> bool {
    base_texture(ctx).is_some()
}

/// 取走本帧的底层画布纹理。
fn base_texture(ctx: &egui::Context) -> Option<egui::TextureId> {
    ctx.data(|d| d.get_temp::<Option<egui::TextureId>>(egui::Id::unique("lumino.base-texture")))
        .flatten()
}

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
    // 底层画布：整窗采样 lumino-gfx 的离屏纹理（取代 gfx → surface 的 blit）。
    // 必须最先画，后面的 band / 面板都在它上面。
    if let Some(id) = base_texture(ui.ctx()) {
        root.painter().image(
            id,
            full,
            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }

    // 本帧菜单占用区先清空：未画菜单时不能残留上一帧的矩形（会吞掉 iced 点击）。
    menu::clear_blocked(ui.ctx());

    // 走带标尺：`LUMINO_EGUI_ARRANGEMENT=1` 时由 egui 绘制（默认关闭，先与 iced 并存）。
    // 几何与 iced `view_arrangement` 同源：内容区左缘 = 左侧栏宽，顶部 = 标题栏 + 工具栏，
    // 标尺再让出 160px 音轨列与 20px 垂直滚动条。
    if arrangement_enabled()
        && let Some(arrangement) = arrangement_model(ui.ctx())
        && let Some(action) = arrangement::show(&root, root.max_rect(), &arrangement)
    {
        arrangement::push_action(ui.ctx(), action);
    }

    // 混音台（浮动面板）：整块由 egui 绘制并接管输入（覆盖 iced 面板）。
    if let Some((layout, mixer)) = mixer_model(ui.ctx()) {
        crate::mixer::show(&root, &layout, &mixer);
        menu::extend_blocked(ui.ctx(), [layout.panel]);
        for action in crate::mixer::interact(&root, &layout, &mixer) {
            push_mixer_action(ui.ctx(), action);
        }
    }

    // 视频剪辑时间轴（渲染器入口面板内）：先默认关闭，仅做与 iced 的像素对照。
    if video_timeline_enabled()
        && let Some((rect, timeline)) = video_timeline(ui.ctx())
    {
        crate::video_clip_timeline::show(&root, rect, &timeline);
        // 时间轴由 egui 接管输入：整块占用，避免 iced 的同位画布再处理一次。
        menu::extend_blocked(ui.ctx(), [rect]);
        for action in crate::video_clip_timeline::interact(&root, rect, &timeline) {
            push_video_timeline_action(ui.ctx(), action);
        }
    }

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
            DialogModel::Help(model) => {
                crate::help::show(&mut root, model);
            }
            DialogModel::CloudConnect(model) => {
                crate::cloud_connect::show(&mut root, model);
            }
            DialogModel::MemoryMonitor(model) => {
                crate::memory_monitor::show(&mut root, model);
            }
            DialogModel::DrawSettings(model) => {
                crate::draw_settings::show(&mut root, model);
            }
            DialogModel::LoadConfirm(model) => {
                crate::load_confirm::show(&mut root, model);
            }
            DialogModel::BrushSettings(model) => {
                crate::brush_settings::show(&mut root, model);
            }
            DialogModel::RecoverTrack(model) => {
                crate::recover_track::show(&mut root, model);
            }
            DialogModel::Collaboration(model) => {
                crate::collaboration::show(&mut root, model);
            }
            DialogModel::CloudBrowser(model) => {
                crate::cloud_browser::show(&mut root, model);
            }
            DialogModel::CloudNotice(model) => {
                crate::cloud_browser::show_notice(&mut root, model);
            }
            DialogModel::VideoExportOverlay(model) => {
                crate::video_export_overlay::show(&mut root, model);
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

/// 走带视图是否已由 egui 接管（`LUMINO_EGUI_ARRANGEMENT=1`；默认关闭）。
/// 走带视图是否由 egui 接管（**默认开启**；`LUMINO_EGUI_ARRANGEMENT=0` 回退 iced）。
///
/// 实测：egui 的泳道点击落点与指针一致，而 iced 的 click canvas 存在
/// canvas 偏移量级（≈208 逻辑像素）的既有偏差；标尺/轨道列在 iced 侧完全不绘制内容。
/// 因此默认走 egui 属于修复，而不是等价切换。
fn arrangement_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("LUMINO_EGUI_ARRANGEMENT")
            .map(|v| v != "0")
            .unwrap_or(true)
    })
}

/// 时间轴动作通道（一帧可多条）。
const VIDEO_TIMELINE_ACTION_ID: &str = "lumino.video-timeline.action";

/// 追加本帧时间轴动作。
fn push_video_timeline_action(
    ctx: &egui::Context,
    action: crate::video_clip_timeline::VideoTimelineAction,
) {
    let mut actions = ctx
        .data(|d| {
            d.get_temp::<Vec<crate::video_clip_timeline::VideoTimelineAction>>(egui::Id::unique(
                VIDEO_TIMELINE_ACTION_ID,
            ))
        })
        .unwrap_or_default();
    actions.push(action);
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(VIDEO_TIMELINE_ACTION_ID), actions));
}

/// 取走本帧时间轴动作。
pub fn take_video_timeline_actions(
    ctx: &egui::Context,
) -> Vec<crate::video_clip_timeline::VideoTimelineAction> {
    ctx.data_mut(|d| {
        d.remove_temp::<Vec<crate::video_clip_timeline::VideoTimelineAction>>(egui::Id::unique(
            VIDEO_TIMELINE_ACTION_ID,
        ))
    })
    .unwrap_or_default()
}

/// 混音台动作通道。
const MIXER_ACTION_ID: &str = "lumino.mixer.action";

fn push_mixer_action(ctx: &egui::Context, action: crate::mixer::MixerAction) {
    let mut actions = ctx
        .data(|d| d.get_temp::<Vec<crate::mixer::MixerAction>>(egui::Id::unique(MIXER_ACTION_ID)))
        .unwrap_or_default();
    actions.push(action);
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(MIXER_ACTION_ID), actions));
}

/// 取走本帧混音台动作。
pub fn take_mixer_actions(ctx: &egui::Context) -> Vec<crate::mixer::MixerAction> {
    ctx.data_mut(|d| {
        d.remove_temp::<Vec<crate::mixer::MixerAction>>(egui::Id::unique(MIXER_ACTION_ID))
    })
    .unwrap_or_default()
}
