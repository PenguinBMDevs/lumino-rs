//! 叠加层动作分发：把 egui 各通道的动作翻译成生产 `Message` 并交给 `Host`。
//!
//! 从 `egui_overlay.rs` 拆出（REF-001：单文件 < 400 行）。动作在 `run_ui` 之后
//! 统一取走，保证一帧内不遗漏、顺序稳定（先控件后弹层）。

use super::SPLITTER_W;
use lumino_ui_egui::audio_export::AudioExportAction;
use lumino_ui_egui::autosave_recover::AutosaveRecoverAction;
use lumino_ui_egui::batch_edit::BatchEditAction;
use lumino_ui_egui::brush_settings::BrushSettingsAction;
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
pub(super) struct OverlayActions {
    pub menu: Option<usize>,
    pub toolbar: Option<ToolbarAction>,
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
    pub(super) fn take(ctx: &egui::Context) -> Self {
        Self {
            menu: lumino_ui_egui::chrome::menu::take_action(ctx),
            toolbar: lumino_ui_egui::chrome::toolbar_popup::take_action(ctx),
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
    /// 应用本帧动作：每个动作最多产生一条 `Message`，按固定顺序处理。
    pub(super) fn apply_overlay_actions(&mut self, actions: OverlayActions, logical_width: f32) {
        if let Some(action) = actions.toast {
            match action {
                lumino_ui_egui::chrome::toast::ToastAction::Dismiss(id) => {
                    self.root.toast.dismiss(id);
                    self.ui_dirty = true;
                }
            }
        }
        if let Some(action) = actions.titlebar {
            let message = self.root.titlebar_action_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.first_screen {
            let message = self.root.first_screen_action_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.piano_roll_menu {
            let message = self.root.piano_roll_context_menu_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.editor_buttons {
            let message = self.root.editor_button_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(bend) = actions.bend_buttons {
            let message = self.root.bend_button_message(bend);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.text_tool {
            let message = self.root.text_tool_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.velocity_panel {
            let message = self.root.velocity_panel_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(directed) = actions.scrollbar {
            let message = match (directed.vertical, directed.action) {
                (true, lumino_ui_egui::scrollbar::ScrollbarAction::Scroll(y)) => {
                    crate::message::Message::ScrollbarScrolledY(y)
                }
                (false, lumino_ui_egui::scrollbar::ScrollbarAction::Scroll(x)) => {
                    crate::message::Message::ScrollbarScrolled(x)
                }
                (true, lumino_ui_egui::scrollbar::ScrollbarAction::Zoom { zoom, fixed_ratio }) => {
                    crate::message::Message::ZoomYChanged { zoom, fixed_ratio }
                }
                (false, lumino_ui_egui::scrollbar::ScrollbarAction::Zoom { zoom, fixed_ratio }) => {
                    crate::message::Message::ZoomXChanged { zoom, fixed_ratio }
                }
            };
            self.dispatch_overlay_message(message);
        }
        if let Some(index) = actions.menu
            && let Some(message) = self.root.menu_action_message(index)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.dialog
            && let Some(message) = self.root.save_confirm_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.project
            && let Some(message) = self.root.project_settings_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.autosave
            && let Some(message) = self.root.autosave_recover_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.history
            && let Some(message) = self.root.history_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.batch_edit
            && let Some(message) = self.root.batch_edit_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if !actions.shape_canvas.is_empty() {
            // 画布动作直接改文档（与 上一代 Canvas 同一套逻辑），不走消息队列。
            let size = self.render_ctx.viewport.logical_size();
            let panel = self.root.state.shape_drawer.panel_width + SPLITTER_W;
            let top = if self.root.settings.synth.use_native_titlebar {
                0.0
            } else {
                lumino_ui_egui::layout::TITLEBAR_H
            };
            let canvas_rect = egui::Rect::from_min_max(
                egui::pos2(panel, top),
                egui::pos2(size.width, size.height),
            );
            for action in &actions.shape_canvas {
                self.root.apply_shape_canvas_action(action, canvas_rect);
            }
            self.ui_dirty = true;
            self.window_ctx.window.request_redraw();
        }
        if let Some(action) = actions.help_tip {
            let message = self.root.help_tip_action_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.sidebar_menu
            && let Some(message) = self.root.sidebar_menu_action_message(action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.sidebar_panel {
            let message = self.root.sidebar_panel_action_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.statusbar {
            let message = self.root.statusbar_action_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.mask_panel {
            let message = self.root.mask_panel_action_message(action);
            self.dispatch_overlay_message(message);
        }
        // DRAW-045：素材删除确认 / 自定义 BPM 上限都是独立窗口，动作只在
        // 各自的对话框 Root 上产生；主窗口不渲染对应视图，不会走到这里。
        if let Some(action) = actions.material_delete {
            let message = self.root.material_delete_action_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.tempo_max_bpm {
            let message = self.root.tempo_max_bpm_action_message(action);
            self.dispatch_overlay_message(message);
        }
        // 混音台：独立窗口内产生的动作先落到对话框 Root，由 Runner 每帧取走并
        // 映射到主窗口；主窗口自身不再绘制混音台，因此不会走消息分发分支。
        for action in &actions.mixer {
            if self.root.state.dialog_type == crate::state::root_state::DialogType::Mixer {
                self.root.apply_mixer_dialog_action(*action);
                self.ui_dirty = true;
                self.window_ctx.window.request_redraw();
            } else if let Some(message) = self.root.mixer_action_message(action) {
                self.dispatch_overlay_message(message);
            }
        }
        for action in &actions.video_timeline {
            let message = self
                .root
                .video_timeline_action_message(action, self.root.state.video_clip.preview_width);
            self.dispatch_overlay_message(message);
        }
        for action in &actions.lane {
            let message = self.root.lane_action_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = &actions.arrangement
            && let Some(message) = self.root.arrangement_action_message(action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.video_overlay
            && let Some(message) = self.root.video_overlay_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(route) = actions.sidebar_route {
            use crate::sidebar::Event as SidebarEvent;
            let event = match route {
                SidebarRoute::PianoRollGroup => {
                    SidebarEvent::GroupToggled(crate::sidebar::GroupId::PianoRoll)
                }
                SidebarRoute::ProjectGroup => {
                    SidebarEvent::GroupToggled(crate::sidebar::GroupId::Project)
                }
                SidebarRoute::WaterfallGroup => {
                    SidebarEvent::GroupToggled(crate::sidebar::GroupId::Waterfall)
                }
                SidebarRoute::RendererGroup => {
                    SidebarEvent::GroupToggled(crate::sidebar::GroupId::Renderer)
                }
                SidebarRoute::FilePanel => SidebarEvent::PanelToggled(crate::sidebar::Route::File),
                SidebarRoute::AutomationPanel => SidebarEvent::AutomationPanelToggled,
                SidebarRoute::VideoExport => {
                    SidebarEvent::RouteUpdated(crate::sidebar::Route::VideoExport)
                }
                SidebarRoute::AudioExport => {
                    SidebarEvent::RouteUpdated(crate::sidebar::Route::AudioExport)
                }
                // 底部按钮：构造器直接返回 `Message`（不是 `Event` 变体），提前分发。
                SidebarRoute::RollBarHorizontal | SidebarRoute::RollBarVertical => {
                    let button = match route {
                        SidebarRoute::RollBarHorizontal => {
                            lumino_ui_core::sidebar_event::RollBarButton::Horizontal
                        }
                        _ => lumino_ui_core::sidebar_event::RollBarButton::Vertical,
                    };
                    self.dispatch_overlay_message(SidebarEvent::roll_bar_toggled(button));
                    return;
                }
                SidebarRoute::MixerToggle => {
                    self.dispatch_overlay_message(SidebarEvent::mixer_panel_toggled());
                    return;
                }
            };
            self.dispatch_overlay_message(crate::message::Message::Sidebar(event));
        }
        if let Some(action) = actions.video_export
            && let Some(message) = self.root.video_export_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.shape_drawer
            && let Some(message) = self.root.shape_drawer_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.cloud_browser
            && let Some(message) = self.root.cloud_browser_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.collaboration
            && let Some(message) = self.root.collaboration_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.recover_track
            && let Some(message) = self.root.recover_track_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.brush_settings
            && let Some(message) = self.root.brush_settings_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.load_confirm
            && let Some(message) = self.root.load_confirm_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.audio_export
            && let Some(message) = self.root.audio_export_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.draw_settings
            && let Some(message) = self.root.draw_settings_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.memory_monitor
            && let Some(message) = self.root.memory_monitor_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.cloud_connect
            && let Some(message) = self.root.cloud_connect_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.help
            && let Some(message) = self.root.help_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.custom_precision
            && let Some(message) = self.root.custom_precision_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.speed_change
            && let Some(message) = self.root.speed_change_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.settings
            && let Some(message) = self.root.settings_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.panel
            && let Some(message) = self.root.right_sidebar_action_message(&action)
        {
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.toolbar {
            // 溢出项需要按「同一视口宽度」重算折叠结果，单独解析。
            let message = match &action {
                ToolbarAction::OverflowItem(index) => {
                    self.root.overflow_action_message(*index, logical_width)
                }
                other => self.root.toolbar_action_message(other),
            };
            if let Some(message) = message {
                self.dispatch_overlay_message(message);
            }
        }
        if let Some(action) = actions.overlay_window {
            use lumino_ui_egui::overlay_window::OverlayWindowAction;
            let message = match action {
                OverlayWindowAction::SuppressToggled => {
                    crate::window::Event::device_warning_suppress_toggled(true)
                }
                OverlayWindowAction::Quit => crate::window::Event::device_warning_actioned(
                    crate::window::DeviceWarningAction::Quit,
                ),
                OverlayWindowAction::Continue { suppress } => {
                    crate::window::Event::device_warning_actioned(
                        crate::window::DeviceWarningAction::Continue { suppress },
                    )
                }
            };
            self.dispatch_overlay_message(message);
        }
    }

    /// 标记重绘并投递消息（所有叠加层动作共用）。
    ///
    /// 这里是全部 egui 动作的唯一出口：需要排查动作分发时开
    /// `RUST_LOG=lumino_ui=trace` 即可，**不要在每条通道里各打一行 info**——
    /// 滚动/拖拽每帧都会产生动作，会把正常日志冲掉。
    fn dispatch_overlay_message(&mut self, message: crate::Message) {
        tracing::trace!(?message, "egui 动作 → 消息");
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
        let _ = self.process_message(message);
    }
}
