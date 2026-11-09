//! 叠加层动作分发：把 egui 各通道的动作翻译成生产 `Message` 并交给 `Host`。
//!
//! 从 `egui_overlay.rs` 拆出（REF-001：单文件 < 400 行）。动作在 `run_ui` 之后
//! 统一取走，保证一帧内不遗漏、顺序稳定（先控件后弹层）。

use lumino_ui_egui::audio_export::AudioExportAction;
use lumino_ui_egui::batch_edit::BatchEditAction;
use lumino_ui_egui::brush_settings::BrushSettingsAction;
use lumino_ui_egui::chrome::ToolbarAction;
use lumino_ui_egui::chrome::right_panel::RightSidebarAction;
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
use lumino_ui_egui::speed_change::SpeedChangeAction;

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
        }
    }
}

impl Host {
    /// 应用本帧动作：每个动作最多产生一条 `Message`，按固定顺序处理。
    pub(super) fn apply_overlay_actions(&mut self, actions: OverlayActions, logical_width: f32) {
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
