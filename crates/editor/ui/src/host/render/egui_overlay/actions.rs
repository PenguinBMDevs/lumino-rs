//! 叠加层动作分发：把 egui 各通道的动作翻译成生产 `Message` 并交给 `Host`。
//!
//! 从 `egui_overlay.rs` 拆出（REF-001：单文件 ≤600 行）。动作在 `run_ui` 之后
//! 统一取走，保证一帧内不遗漏、顺序稳定（先控件后弹层）。

use lumino_ui_egui::audio_export::AudioExportAction;
use lumino_ui_egui::autosave_recover::AutosaveRecoverAction;
use lumino_ui_egui::batch_edit::BatchEditAction;
use lumino_ui_egui::brush_settings::BrushSettingsAction;
use lumino_ui_egui::chrome::DashboardBlur;
use lumino_ui_egui::chrome::ToolbarAction;
use lumino_ui_egui::chrome::arrangement::ArrangementAction;
use lumino_ui_egui::chrome::right_panel::RightSidebarAction;
use lumino_ui_egui::chrome::sidebar::{SidebarRoute, take_route_action};
use lumino_ui_egui::cloud_browser::CloudBrowserAction;
use lumino_ui_egui::cloud_connect::CloudConnectAction;
use lumino_ui_egui::collaboration::CollaborationAction;
use lumino_ui_egui::custom_precision::CustomPrecisionAction;
use lumino_ui_egui::dialog::SaveConfirmAction;
use lumino_ui_egui::draw_settings::DrawSettingsAction;
use lumino_ui_egui::help::HelpAction;
use lumino_ui_egui::history::HistoryAction;
use lumino_ui_egui::load_confirm::LoadConfirmAction;
use lumino_ui_egui::memory_monitor::MemoryMonitorAction;
use lumino_ui_egui::project_settings::ProjectSettingsAction;
use lumino_ui_egui::recover_track::RecoverTrackAction;
use lumino_ui_egui::settings::SettingsAction;
use lumino_ui_egui::shape_canvas::ShapeCanvasAction;
use lumino_ui_egui::shape_drawer::ShapeDrawerAction;
use lumino_ui_egui::speed_change::SpeedChangeAction;
use lumino_ui_egui::video_export::VideoExportAction;
use lumino_ui_egui::video_export_overlay::VideoOverlayAction;

use crate::host::Host;

/// 一帧内 egui 各通道产生的动作集合。
///
/// 生产 `Host` 与原生运行时（`lumino-ui-runtime`）共用这张表；
/// 新增通道只需在 [`Self::take`] 里加一行。
#[derive(Default)]
pub struct OverlayActions {
    pub menu: Option<usize>,
    pub toolbar: Option<ToolbarAction>,
    /// 仪表盘输入框失焦信号（独立于单槽工具栏动作，见 `DashboardBlur`）。
    pub dashboard_blur: Option<DashboardBlur>,
    /// 行内重命名输入框失焦信号（独立于单槽面板动作，见 `SidebarRenameBlur`）。
    pub sidebar_rename_blur: Option<lumino_ui_egui::chrome::SidebarRenameBlur>,
    pub panel: Option<RightSidebarAction>,
    pub settings: Option<SettingsAction>,
    pub dialog: Option<SaveConfirmAction>,
    pub project: Option<ProjectSettingsAction>,
    pub autosave: Option<AutosaveRecoverAction>,
    pub history: Option<HistoryAction>,
    pub batch_edit: Option<BatchEditAction>,
    pub speed_change: Option<SpeedChangeAction>,
    pub custom_precision: Option<CustomPrecisionAction>,
    pub help: Option<HelpAction>,
    pub cloud_connect: Option<CloudConnectAction>,
    pub memory_monitor: Option<MemoryMonitorAction>,
    pub draw_settings: Option<DrawSettingsAction>,
    pub audio_export: Option<AudioExportAction>,
    pub load_confirm: Option<LoadConfirmAction>,
    pub brush_settings: Option<BrushSettingsAction>,
    pub recover_track: Option<RecoverTrackAction>,
    pub collaboration: Option<CollaborationAction>,
    pub cloud_browser: Option<CloudBrowserAction>,
    pub shape_drawer: Option<ShapeDrawerAction>,
    pub shape_canvas: Vec<ShapeCanvasAction>,
    pub video_export: Option<VideoExportAction>,
    pub sidebar_route: Option<SidebarRoute>,
    pub video_overlay: Option<VideoOverlayAction>,
    pub arrangement: Option<ArrangementAction>,
    pub lane: Vec<lumino_message::arrangement_action::ArrangementAction>,
    pub video_timeline: Vec<lumino_ui_egui::video_clip_timeline::VideoTimelineAction>,
    pub mixer: Vec<lumino_ui_egui::mixer::MixerAction>,
    pub material_delete: Option<lumino_ui_egui::material_delete::MaterialDeleteAction>,
    pub tempo_max_bpm: Option<lumino_ui_egui::tempo_max_bpm::TempoMaxBpmAction>,
    pub mask_panel: Option<lumino_ui_egui::mask_panel::MaskPanelAction>,
    pub statusbar: Option<lumino_ui_egui::chrome::statusbar::StatusbarAction>,
    pub sidebar_panel: Option<lumino_ui_egui::chrome::SidebarPanelAction>,
    pub sidebar_menu: Option<lumino_ui_egui::chrome::SidebarMenuAction>,
    pub help_tip: Option<lumino_ui_egui::chrome::toast::HelpTipAction>,
    pub toast: Option<lumino_ui_egui::chrome::toast::ToastAction>,
    pub titlebar: Option<lumino_ui_egui::chrome::TitlebarAction>,
    /// 「无面板」第一屏右栏操作按钮。
    pub first_screen: Option<lumino_ui_egui::chrome::first_screen::FirstScreenAction>,
    pub piano_roll_menu: Option<lumino_message::PianoRollContextMenuAction>,
    pub editor_buttons: Option<lumino_ui_egui::editor_action_buttons::EditorButtonAction>,
    pub bend_buttons: Option<lumino_editor_state::AnchorBend>,
    pub text_tool: Option<lumino_ui_egui::text_tool::TextToolAction>,
    pub velocity_panel: Option<lumino_ui_egui::velocity_panel::VelocityPanelAction>,
    pub scrollbar: Option<lumino_ui_egui::scrollbar::DirectedAction>,
    /// 进度窗 / 设备警告窗动作。
    pub overlay_window: Option<lumino_ui_egui::overlay_window::OverlayWindowAction>,
}

impl OverlayActions {
    /// 从 egui 内存取走本帧全部动作。
    pub fn take(ctx: &egui::Context) -> Self {
        Self {
            menu: lumino_ui_egui::chrome::menu::take_action(ctx),
            toolbar: lumino_ui_egui::chrome::toolbar_popup::take_action(ctx),
            dashboard_blur: lumino_ui_egui::chrome::toolbar::take_dashboard_blur(ctx),
            panel: lumino_ui_egui::chrome::right_panel::take_action(ctx),
            settings: lumino_ui_egui::settings::take_action(ctx),
            dialog: lumino_ui_egui::dialog::take_action(ctx),
            project: lumino_ui_egui::project_settings::take_action(ctx),
            autosave: lumino_ui_egui::autosave_recover::take_action(ctx),
            history: lumino_ui_egui::history::take_action(ctx),
            batch_edit: lumino_ui_egui::batch_edit::take_action(ctx),
            speed_change: lumino_ui_egui::speed_change::take_action(ctx),
            custom_precision: lumino_ui_egui::custom_precision::take_action(ctx),
            help: lumino_ui_egui::help::take_action(ctx),
            cloud_connect: lumino_ui_egui::cloud_connect::take_action(ctx),
            memory_monitor: lumino_ui_egui::memory_monitor::take_action(ctx),
            draw_settings: lumino_ui_egui::draw_settings::take_action(ctx),
            audio_export: lumino_ui_egui::audio_export::take_action(ctx),
            load_confirm: lumino_ui_egui::load_confirm::take_action(ctx),
            brush_settings: lumino_ui_egui::brush_settings::take_action(ctx),
            recover_track: lumino_ui_egui::recover_track::take_action(ctx),
            collaboration: lumino_ui_egui::collaboration::take_action(ctx),
            cloud_browser: lumino_ui_egui::cloud_browser::take_action(ctx),
            shape_drawer: lumino_ui_egui::shape_drawer::take_action(ctx),
            shape_canvas: lumino_ui_egui::shape_canvas::take_actions(ctx),
            video_export: lumino_ui_egui::video_export::take_action(ctx),
            sidebar_route: take_route_action(ctx),
            video_overlay: lumino_ui_egui::video_export_overlay::take_action(ctx),
            arrangement: lumino_ui_egui::chrome::arrangement::take_action(ctx),
            lane: lumino_ui_egui::chrome::arrangement::take_lane_actions(ctx),
            video_timeline: lumino_ui_egui::chrome::take_video_timeline_actions(ctx),
            mixer: lumino_ui_egui::mixer::take_actions(ctx),
            material_delete: lumino_ui_egui::material_delete::take_action(ctx),
            tempo_max_bpm: lumino_ui_egui::tempo_max_bpm::take_action(ctx),
            mask_panel: lumino_ui_egui::chrome::take_mask_panel_action(ctx),
            statusbar: lumino_ui_egui::chrome::statusbar::take_action(ctx),
            sidebar_panel: lumino_ui_egui::chrome::take_panel_action(ctx),
            sidebar_rename_blur: lumino_ui_egui::chrome::take_rename_blur(ctx),
            sidebar_menu: lumino_ui_egui::chrome::take_menu_action(ctx),
            help_tip: lumino_ui_egui::chrome::take_help_tip_action(ctx),
            toast: lumino_ui_egui::chrome::toast::take_toast_action(ctx),
            titlebar: lumino_ui_egui::chrome::take_titlebar_action(ctx),
            first_screen: lumino_ui_egui::chrome::first_screen::take_action(ctx),
            piano_roll_menu: lumino_ui_egui::piano_roll_context_menu::take_action(ctx),
            editor_buttons: lumino_ui_egui::editor_action_buttons::take_action(ctx),
            bend_buttons: lumino_ui_egui::bend_buttons::take_action(ctx),
            text_tool: lumino_ui_egui::text_tool::take_action(ctx),
            velocity_panel: lumino_ui_egui::velocity_panel::take_action(ctx),
            scrollbar: lumino_ui_egui::scrollbar::take_action(ctx),
            overlay_window: lumino_ui_egui::overlay_window::take_action(ctx),
        }
    }
}

impl Host {
    /// 应用本帧动作：翻译由 `Root::plan_egui_actions` 统一完成，
    /// 本方法只决定消息如何落地（`process_message` 负责窗口 / 主题 / 编辑器路由）。
    pub(super) fn apply_overlay_actions(&mut self, actions: OverlayActions, logical_width: f32) {
        let size = self.render_ctx.viewport.logical_size();
        let logical = egui::vec2(logical_width, size.height);
        let mut plan = self.root.plan_egui_actions(actions, logical);
        let messages = std::mem::take(&mut plan.messages);
        for message in messages {
            self.dispatch_overlay_message(message);
        }
        if self.root.apply_egui_mutations(&mut plan) > 0 {
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        }
    }

    /// 把动作应用后的消息投递到生产处理器（含窗口 / 主题路由）。
    fn dispatch_overlay_message(&mut self, message: crate::Message) {
        tracing::trace!(?message, "egui 动作 → 消息");
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
        let _ = self.process_message(message);
    }
}
