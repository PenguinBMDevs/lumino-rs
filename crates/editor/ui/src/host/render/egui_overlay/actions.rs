//! 叠加层动作分发：把 egui 各通道的动作翻译成生产 `Message` 并交给 `Host`。
//!
//! 从 `egui_overlay.rs` 拆出（REF-001：单文件 < 400 行）。动作在 `run_ui` 之后
//! 统一取走，保证一帧内不遗漏、顺序稳定（先控件后弹层）。

use super::SPLITTER_W;
use lumino_ui_egui::audio_export::AudioExportAction;
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
    pub mask_panel: Option<lumino_ui_egui::mask_panel::MaskPanelAction>,
    pub statusbar: Option<lumino_ui_egui::chrome::statusbar::StatusbarAction>,
    pub sidebar_panel: Option<lumino_ui_egui::chrome::SidebarPanelAction>,
    pub sidebar_menu: Option<lumino_ui_egui::chrome::SidebarMenuAction>,
    pub help_tip: Option<lumino_ui_egui::chrome::toast::HelpTipAction>,
    pub titlebar: Option<lumino_ui_egui::chrome::TitlebarAction>,
    pub piano_roll_menu: Option<lumino_message::PianoRollContextMenuAction>,
    pub editor_buttons: Option<lumino_ui_egui::editor_action_buttons::EditorButtonAction>,
    pub bend_buttons: Option<lumino_editor_state::AnchorBend>,
    pub text_tool: Option<lumino_ui_egui::text_tool::TextToolAction>,
    pub velocity_panel: Option<lumino_ui_egui::velocity_panel::VelocityPanelAction>,
    pub scrollbar: Option<lumino_ui_egui::scrollbar::DirectedAction>,
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
            mixer: lumino_ui_egui::chrome::take_mixer_actions(ctx),
            material_delete: lumino_ui_egui::chrome::take_material_delete_action(ctx),
            mask_panel: lumino_ui_egui::chrome::take_mask_panel_action(ctx),
            statusbar: lumino_ui_egui::chrome::statusbar::take_action(ctx),
            sidebar_panel: lumino_ui_egui::chrome::take_panel_action(ctx),
            sidebar_menu: lumino_ui_egui::chrome::take_menu_action(ctx),
            help_tip: lumino_ui_egui::chrome::take_help_tip_action(ctx),
            titlebar: lumino_ui_egui::chrome::take_titlebar_action(ctx),
            piano_roll_menu: lumino_ui_egui::piano_roll_context_menu::take_action(ctx),
            editor_buttons: lumino_ui_egui::editor_action_buttons::take_action(ctx),
            bend_buttons: lumino_ui_egui::bend_buttons::take_action(ctx),
            text_tool: lumino_ui_egui::text_tool::take_action(ctx),
            velocity_panel: lumino_ui_egui::velocity_panel::take_action(ctx),
            scrollbar: lumino_ui_egui::scrollbar::take_action(ctx),
        }
    }
}

impl Host {
    /// 应用本帧动作：每个动作最多产生一条 `Message`，按固定顺序处理。
    pub(super) fn apply_overlay_actions(&mut self, actions: OverlayActions, logical_width: f32) {
        if let Some(action) = actions.titlebar {
            tracing::info!("egui 标题栏动作：{action:?}");
            let message = self.root.titlebar_action_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.piano_roll_menu {
            tracing::info!("egui 卷帘右键菜单：{action:?}");
            let message = self.root.piano_roll_context_menu_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.editor_buttons {
            tracing::info!("egui 编辑器按钮：{action:?}");
            let message = self.root.editor_button_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(bend) = actions.bend_buttons {
            tracing::info!("egui 弯折按钮：{bend:?}");
            let message = self.root.bend_button_message(bend);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.text_tool {
            tracing::info!("egui 文字工具：{action:?}");
            let message = self.root.text_tool_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.velocity_panel {
            tracing::info!("egui 力度面板：{action:?}");
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
            tracing::info!("egui 滚动条：{directed:?}");
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
            tracing::info!("egui 保存确认动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.project
            && let Some(message) = self.root.project_settings_action_message(&action)
        {
            tracing::info!("egui 工程设置动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.history
            && let Some(message) = self.root.history_action_message(&action)
        {
            tracing::info!("egui 撤销历史动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.batch_edit
            && let Some(message) = self.root.batch_edit_action_message(&action)
        {
            tracing::info!("egui 批量编辑动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if !actions.shape_canvas.is_empty() {
            // 画布动作直接改文档（与 iced Canvas 同一套逻辑），不走消息队列。
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
            tracing::info!("egui 帮助提示动作：{action:?}");
            let message = self.root.help_tip_action_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.sidebar_menu
            && let Some(message) = self.root.sidebar_menu_action_message(action)
        {
            tracing::info!("egui 音轨菜单动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.sidebar_panel {
            tracing::info!("egui 音轨列表动作：{action:?}");
            let message = self.root.sidebar_panel_action_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.statusbar {
            tracing::info!("egui 状态栏动作：{action:?}");
            let message = self.root.statusbar_action_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.mask_panel {
            tracing::info!("egui 遮罩面板动作：{action:?}");
            let message = self.root.mask_panel_action_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.material_delete {
            tracing::info!("egui 素材删除动作：{action:?}");
            let message = self.root.material_delete_action_message(action);
            self.dispatch_overlay_message(message);
        }
        for action in &actions.mixer {
            if let Some(message) = self.root.mixer_action_message(action) {
                tracing::info!("egui 混音台动作：{action:?}");
                self.dispatch_overlay_message(message);
            }
        }
        for action in &actions.video_timeline {
            tracing::info!("egui 时间轴动作：{action:?}");
            let message = self
                .root
                .video_timeline_action_message(action, self.root.state.video_clip.preview_width);
            self.dispatch_overlay_message(message);
        }
        for action in &actions.lane {
            tracing::info!("egui 泳道动作：{action:?}");
            let message = self.root.lane_action_message(action);
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = &actions.arrangement
            && let Some(message) = self.root.arrangement_action_message(action)
        {
            tracing::info!("egui 走带动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.video_overlay
            && let Some(message) = self.root.video_overlay_action_message(&action)
        {
            tracing::info!("egui 视频导出覆盖层动作：{action:?}");
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
                    tracing::info!("egui 路由栏动作：{route:?}");
                    self.dispatch_overlay_message(SidebarEvent::roll_bar_toggled(button));
                    return;
                }
                SidebarRoute::MixerToggle => {
                    tracing::info!("egui 路由栏动作：{route:?}");
                    self.dispatch_overlay_message(SidebarEvent::mixer_panel_toggled());
                    return;
                }
            };
            self.dispatch_overlay_message(crate::message::Message::Sidebar(event));
        }
        if let Some(action) = actions.video_export
            && let Some(message) = self.root.video_export_action_message(&action)
        {
            tracing::info!("egui 视频导出动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.shape_drawer
            && let Some(message) = self.root.shape_drawer_action_message(&action)
        {
            tracing::info!("egui 形状抽屉动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.cloud_browser
            && let Some(message) = self.root.cloud_browser_action_message(&action)
        {
            tracing::info!("egui 云浏览动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.collaboration
            && let Some(message) = self.root.collaboration_action_message(&action)
        {
            tracing::info!("egui 协作动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.recover_track
            && let Some(message) = self.root.recover_track_action_message(&action)
        {
            tracing::info!("egui 找回音轨动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.brush_settings
            && let Some(message) = self.root.brush_settings_action_message(&action)
        {
            tracing::info!("egui 画刷设置动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.load_confirm
            && let Some(message) = self.root.load_confirm_action_message(&action)
        {
            tracing::info!("egui 载入确认动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.audio_export
            && let Some(message) = self.root.audio_export_action_message(&action)
        {
            tracing::info!("egui 音频导出动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.draw_settings
            && let Some(message) = self.root.draw_settings_action_message(&action)
        {
            tracing::info!("egui 音符画设置动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.memory_monitor
            && let Some(message) = self.root.memory_monitor_action_message(&action)
        {
            tracing::info!("egui 内存监控动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.cloud_connect
            && let Some(message) = self.root.cloud_connect_action_message(&action)
        {
            tracing::info!("egui 云连接动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.help
            && let Some(message) = self.root.help_action_message(&action)
        {
            tracing::info!("egui 帮助动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.custom_precision
            && let Some(message) = self.root.custom_precision_action_message(&action)
        {
            tracing::info!("egui 自定义精度动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.speed_change
            && let Some(message) = self.root.speed_change_action_message(&action)
        {
            tracing::info!("egui 变速动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.settings
            && let Some(message) = self.root.settings_action_message(&action)
        {
            tracing::info!("egui 设置动作：{action:?}");
            self.dispatch_overlay_message(message);
        }
        if let Some(action) = actions.panel
            && let Some(message) = self.root.right_sidebar_action_message(&action)
        {
            tracing::info!("egui 右侧栏动作：{action:?}");
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
                tracing::info!("egui 工具栏动作：{action:?}");
                self.dispatch_overlay_message(message);
            }
        }
    }

    /// 标记重绘并投递消息（所有叠加层动作共用）。
    fn dispatch_overlay_message(&mut self, message: crate::Message) {
        self.ui_dirty = true;
        self.window_ctx.window.request_redraw();
        let _ = self.process_message(message);
    }
}
