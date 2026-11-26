//! 主窗口外壳：标题栏 + 工具栏 + 左右侧栏 + 状态栏 + 钢琴卷帘骨架。
//!
//! 本文件只保留模块声明、公开 re-export 与整窗入口；band 编排在 [`bands`]，
//! 覆盖层在 [`overlays`]，对话框接管在 [`dialog_layer`]，`ctx.data` 通道与
//! 窗口几何在 [`channels`]。

pub mod arrangement;
mod bands;
mod channels;
mod dialog_layer;
pub mod egg;
pub mod first_screen;
mod floating;
pub mod menu;
mod model;
pub mod overflow;
mod overlays;
pub mod piano_roll;
pub mod popup_titlebar;
pub mod right_panel;
pub mod sidebar;
pub mod statusbar;
pub mod titlebar;
pub mod toast;
pub mod tool_panel;
pub mod toolbar;
pub mod toolbar_popup;

pub use arrangement::ArrangementAction;
pub use bands::show_bands;
pub use channels::{
    base_texture_active, set_area_palette, set_arrangement, set_base_texture, set_bend_buttons,
    set_curve_preview, set_editor_buttons, set_editor_overlay, set_egg, set_first_screen,
    set_help_tip, set_i2m_region, set_marquee, set_mask_panel, set_piano_roll_menu,
    set_remote_cursors, set_renderer_panel, set_shape_edit, set_shape_list, set_shape_preview,
    set_text_tool, set_toast, set_velocity_panel, set_video_preview, set_video_timeline,
    set_window_size, take_help_tip_action, take_mask_panel_action, take_video_timeline_actions,
};
pub(crate) use channels::{push_video_timeline_action, window_size};
pub use egg::EggModel;
pub(crate) use floating::floating_panel;
pub use model::{
    AboutModel, BatchEditFieldModel, BatchEditModel, BatchEditRowModel, BrushLevelModel,
    BrushSettingsModel, BrushTrackChoice, ChromeModel, CloudBrowserModel, CloudConnectModel,
    CloudDeviceModel, CloudEntryActionKind, CloudEntryLabels, CloudEntryModel, CloudField,
    CloudFieldModel, CloudNoticeModel, CloudProtocol, CloudProtocolOptionModel,
    CloudSavePanelModel, CollaborationConnectView, CollaborationConnectingView,
    CollaborationInRoomView, CollaborationModel, CollaborationRoomActionsView,
    CollaborationViewModel, CompatLine, CompatibilityPageModel, CustomPrecisionModel,
    DeviceWarningModel, DialogModel, DialogPalette, DotOptionModel, DrawSettingsModel,
    ExportMessageKind, ExportProgressModel, HelpModel, HelpSectionModel, HelpTopicBodyModel,
    HelpTopicRowModel, HistoryModel, HistoryRowModel, LoadConfirmModel, MaterialDeleteModel,
    MemoryMetricModel, MemoryMonitorModel, MemoryRowModel, MenuBarModel, MenuEntry, MenuModel,
    OnionMenuItem, OnionMenuModel, OnionSkinPageModel, OverflowItemModel, OverflowMenuModel,
    ProgressModel, ProjectNoteCountModel, ProjectSettingsModel, RENDER_PREVIEW_TEXTURE_ID,
    RecoverTrackModel, RecoverTrackRowModel, RecoverTrackSelection, RightSidebarModel,
    RightSidebarPanelKind, RightSidebarRailItem, RollBar, SaveConfirmModel, SettingsLabels,
    SettingsPageModel, SettingsPalette, SettingsSnapshot, ShapeActionButton, ShapeDrawerModel,
    ShapePanelAction, ShapeRowModel, ShapeSwatchModel, ShortcutsPageModel, SidebarGroup,
    SidebarModel, SidebarPanel, SidebarRouteIcon, SidebarRouteItem, SidebarTrack, SnapMenuItem,
    SnapMenuModel, SpeedChangeModel, StatusbarModel, TempoMaxBpmModel, TextPairCell,
    TexturePreviewModel, TitlebarModel, ToolbarGroupsModel, ToolbarModel, ToolbarTips, VideoButton,
    VideoButtonFill, VideoExportAction, VideoExportModel, VideoExportOverlayModel,
    VideoExportPanelModel, VideoOverlayAction, VideoOverlayCompleted, VideoOverlayError,
    VideoOverlayFinalizing, VideoOverlayProgress, VideoOverlayView, VideoPreviewFrame, VideoRow,
    VideoSection,
};
pub use model::{
    ArrangementLaneModel, ArrangementModel, ArrangementRulerModel, ArrangementScrollbarsModel,
    ArrangementTrackListModel, ArrangementTrackRow, AudioButtonsMode, AudioButtonsModel,
    AudioChoice, AudioControlModel, AudioExportModel, AudioExportPanelModel, AudioField,
    AudioFilterModel, AudioOutputModel, AudioPageModel, AudioProjectModel, AudioRowModel,
    AudioSettingsModel, AudioToggle, AudioToggleId, AutosaveRecoverModel, BrushPanelModel,
    CloudConnModel, CloudPageModel, EDITOR_TEXTURE_ID, FillDivisionPanelModel, GpuMetricsModel,
    HelpTipModel, I2mControlModel, I2mConvertModel, I2mField, I2mPanelModel, I2mRowModel,
    I2mSelectField, MaterialContextLabels, MaterialContextMenuItem, MaterialContextMenuModel,
    MaterialItemModel, MaterialMenuEntryModel, MaterialMenuKind, MaterialSectionModel,
    MaterialsPanelModel, MetricTone, RightSidebarPanelModel, ShapeKind, ShapePanelModel,
    SidebarColorPickerModel, SidebarMenuEntry, SidebarNumericPickerKind, SidebarNumericPickerModel,
    SidebarPanelMenuModel, SidebarReorderModel, SidebarTrackMenuModel, TextParamsPanelModel,
    ThemeCardModel, ThicknessPanelModel, ToastCardModel, ToastModel, ToolPanelItemKind,
    ToolPanelItemModel, ToolPanelModel, ToolPanelPanelOwner, ToolPanelSettingsPanel, UiField,
    UiPageModel, UiRow, WATERFALL_TEXTURE_ID, WaterfallPanelModel, governor_tone, load_tone,
    underrun_tone,
};
pub use popup_titlebar::PopupTitlebarModel;
pub use sidebar::{
    RenameBlur, SidebarMenuAction, SidebarPanelAction, SidebarRenameBlur, take_menu_action,
    take_panel_action, take_rename_blur,
};
pub use titlebar::{TitlebarAction, take_titlebar_action};
pub use toolbar::{DashboardBlur, DashboardField, MetricBlur};
pub use toolbar_popup::{ToolbarAction, ToolbarCommand};

use egui::{Frame, Margin, Panel, Stroke};

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
                // 上一代 PianoRollSkeleton：zoom_x=0.05、滚动到第 9 小节。
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

    // 生产 上一代 结构：`column![toolbar, row![editor, right_bar]]`——
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

pub(crate) fn panel_frame(fill: egui::Color32) -> Frame {
    Frame::default()
        .fill(fill)
        .inner_margin(Margin::ZERO)
        .outer_margin(Margin::ZERO)
        .stroke(Stroke::new(0.0, egui::Color32::TRANSPARENT))
}
