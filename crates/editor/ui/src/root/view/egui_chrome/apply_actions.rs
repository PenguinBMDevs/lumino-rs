//! 原生运行时把 egui 动作表应用为 `Message`。
//!
//! 通道表的唯一来源是 [`OverlayActions`]（生产 `Host` 与原生运行时共用）。
//! 生产侧仍由 `Host::apply_overlay_actions` 消费，因为 `Host::process_message`
//! 还负责窗口 / 主题 / 编辑器动作路由；本文件只做 Root 能自行消化的部分。
//! 未覆盖的通道必须显式处理，不允许再出现 `let _ =` 式静默丢弃。

use lumino_ui_egui::chrome::sidebar::SidebarRoute;
use lumino_ui_egui::scrollbar::ScrollbarAction;

use crate::Message;
use crate::host::OverlayActions;
use crate::root::Root;
use crate::state::root_state::DialogType;

/// 取一个 `Option<T>` 通道，经映射器转成 `Message` 后应用。
///
/// 这是本文件唯一的分发形状：新增通道只需一行，映射器返回 `None`
/// （状态未变化）时不计数。
/// 恒返回 `Message` 的通道。
macro_rules! apply_always {
    ($self:ident, $actions:ident, $applied:ident, $field:ident, $mapper:ident) => {{
        if let Some(action) = $actions.$field.take() {
            $self.update($self.$mapper(action));
            *$applied += 1;
        }
    }};
}

/// 返回 `Option<Message>` 的通道（状态未变化时不计入）。
macro_rules! apply_option {
    ($self:ident, $actions:ident, $applied:ident, $field:ident, $mapper:ident) => {{
        if let Some(action) = $actions.$field.take()
            && let Some(message) = $self.$mapper(action)
        {
            $self.update(message);
            *$applied += 1;
        }
    }};
}

/// 同 [`apply_option`]，映射器借用动作。
macro_rules! apply_option_ref {
    ($self:ident, $actions:ident, $applied:ident, $field:ident, $mapper:ident) => {{
        if let Some(action) = $actions.$field.take()
            && let Some(message) = $self.$mapper(&action)
        {
            $self.update(message);
            *$applied += 1;
        }
    }};
}

impl Root {
    /// 应用本帧全部 egui 动作，返回实际生效的条数。
    pub fn apply_egui_actions(&mut self, actions: OverlayActions, logical: egui::Vec2) -> usize {
        let mut actions = actions;
        let mut applied = 0;
        self.apply_egui_blurs(&mut actions, &mut applied);
        self.apply_egui_before_canvas(&mut actions, logical, &mut applied);
        applied
    }

    /// 输入框失焦与通知：必须抢在其它动作之前收口。
    fn apply_egui_blurs(&mut self, actions: &mut OverlayActions, applied: &mut usize) {
        use lumino_ui_egui::chrome::toolbar::{DashboardField, MetricBlur};

        if let Some(blur) = actions.dashboard_blur.take() {
            let message = match (blur.field, blur.outcome) {
                (DashboardField::Bpm, MetricBlur::Commit) => {
                    crate::toolbar::Event::bpm_edit_confirmed()
                }
                (DashboardField::Bpm, MetricBlur::Cancel) => {
                    crate::toolbar::Event::bpm_edit_toggled(self.dashboard_bpm())
                }
                (DashboardField::Ppq, MetricBlur::Commit) => {
                    crate::toolbar::Event::ppq_edit_confirmed()
                }
                (DashboardField::Ppq, MetricBlur::Cancel) => {
                    crate::toolbar::Event::ppq_edit_toggled(self.editor.editor_state.view.ppq)
                }
            };
            self.update(message);
            *applied += 1;
        }
        if let Some(blur) = actions.sidebar_rename_blur.take() {
            for message in self.sidebar_rename_blur_messages(blur) {
                self.update(message);
                *applied += 1;
            }
        }
        if let Some(action) = actions.toast.take()
            && let lumino_ui_egui::chrome::toast::ToastAction::Dismiss(id) = action
        {
            self.toast.dismiss(id);
            *applied += 1;
        }
    }

    /// 画布动作之前的通道（工具栏 / 菜单 / 对话框）。
    fn apply_egui_before_canvas(
        &mut self,
        actions: &mut OverlayActions,
        logical: egui::Vec2,
        applied: &mut usize,
    ) {
        apply_always!(self, actions, applied, titlebar, titlebar_action_message);
        apply_always!(
            self,
            actions,
            applied,
            first_screen,
            first_screen_action_message
        );
        apply_always!(
            self,
            actions,
            applied,
            piano_roll_menu,
            piano_roll_context_menu_message
        );
        apply_always!(
            self,
            actions,
            applied,
            editor_buttons,
            editor_button_message
        );
        apply_always!(self, actions, applied, bend_buttons, bend_button_message);
        apply_always!(self, actions, applied, text_tool, text_tool_message);
        apply_always!(
            self,
            actions,
            applied,
            velocity_panel,
            velocity_panel_message
        );
        if let Some(directed) = actions.scrollbar.take() {
            let message = match (directed.vertical, directed.action) {
                (true, ScrollbarAction::Scroll(y)) => Message::ScrollbarScrolledY(y),
                (false, ScrollbarAction::Scroll(x)) => Message::ScrollbarScrolled(x),
                (true, ScrollbarAction::Zoom { zoom, fixed_ratio }) => {
                    Message::ZoomYChanged { zoom, fixed_ratio }
                }
                (false, ScrollbarAction::Zoom { zoom, fixed_ratio }) => {
                    Message::ZoomXChanged { zoom, fixed_ratio }
                }
            };
            self.update(message);
            *applied += 1;
        }
        apply_option!(self, actions, applied, menu, menu_action_message);
        apply_option_ref!(self, actions, applied, dialog, save_confirm_action_message);
        apply_option_ref!(
            self,
            actions,
            applied,
            project,
            project_settings_action_message
        );
        apply_option_ref!(
            self,
            actions,
            applied,
            autosave,
            autosave_recover_action_message
        );
        apply_option_ref!(self, actions, applied, history, history_action_message);
        apply_option_ref!(
            self,
            actions,
            applied,
            batch_edit,
            batch_edit_action_message
        );
        let canvas_actions = std::mem::take(&mut actions.shape_canvas);
        self.apply_shape_canvas_actions(&canvas_actions, logical, applied);
        self.apply_egui_after_canvas(actions, logical, applied);
    }

    /// 画布动作之后的通道（面板 / 列表 / 独立对话框）。
    /// 画布动作之后的通道（面板 / 通知）。
    fn apply_egui_after_canvas(
        &mut self,
        actions: &mut OverlayActions,
        logical: egui::Vec2,
        applied: &mut usize,
    ) {
        self.apply_egui_panel_channels(actions, applied);
        self.apply_egui_list_channels(actions, applied);
        self.apply_egui_window_channels(actions, logical, applied);
    }

    fn apply_egui_panel_channels(&mut self, actions: &mut OverlayActions, applied: &mut usize) {
        apply_always!(self, actions, applied, help_tip, help_tip_action_message);
        apply_option!(
            self,
            actions,
            applied,
            sidebar_menu,
            sidebar_menu_action_message
        );
        apply_always!(
            self,
            actions,
            applied,
            sidebar_panel,
            sidebar_panel_action_message
        );
        apply_always!(self, actions, applied, statusbar, statusbar_action_message);
        apply_always!(
            self,
            actions,
            applied,
            mask_panel,
            mask_panel_action_message
        );
        apply_always!(
            self,
            actions,
            applied,
            material_delete,
            material_delete_action_message
        );
        apply_always!(
            self,
            actions,
            applied,
            tempo_max_bpm,
            tempo_max_bpm_action_message
        );
    }

    fn apply_egui_list_channels(&mut self, actions: &mut OverlayActions, applied: &mut usize) {
        for action in std::mem::take(&mut actions.mixer) {
            if self.state.dialog_type == DialogType::Mixer {
                self.apply_mixer_dialog_action(action);
                *applied += 1;
            } else if let Some(message) = self.mixer_action_message(&action) {
                self.update(message);
                *applied += 1;
            }
        }
        for action in std::mem::take(&mut actions.video_timeline) {
            let message =
                self.video_timeline_action_message(&action, self.state.video_clip.preview_width);
            self.update(message);
            *applied += 1;
        }
        for action in std::mem::take(&mut actions.lane) {
            let message = self.lane_action_message(&action);
            self.update(message);
            *applied += 1;
        }
        apply_option_ref!(
            self,
            actions,
            applied,
            arrangement,
            arrangement_action_message
        );
        apply_option_ref!(
            self,
            actions,
            applied,
            video_overlay,
            video_overlay_action_message
        );
        self.apply_sidebar_route(actions.sidebar_route.take(), applied);
    }

    /// 独立对话框与窗口级通道。
    fn apply_egui_window_channels(
        &mut self,
        actions: &mut OverlayActions,
        logical: egui::Vec2,
        applied: &mut usize,
    ) {
        self.apply_egui_dialog_channels(actions, applied);
        self.apply_egui_window_tail(actions, logical, applied);
    }

    fn apply_egui_dialog_channels(&mut self, actions: &mut OverlayActions, applied: &mut usize) {
        apply_option_ref!(
            self,
            actions,
            applied,
            video_export,
            video_export_action_message
        );
        apply_option_ref!(
            self,
            actions,
            applied,
            shape_drawer,
            shape_drawer_action_message
        );
        apply_option_ref!(
            self,
            actions,
            applied,
            cloud_browser,
            cloud_browser_action_message
        );
        apply_option_ref!(
            self,
            actions,
            applied,
            collaboration,
            collaboration_action_message
        );
        apply_option_ref!(
            self,
            actions,
            applied,
            recover_track,
            recover_track_action_message
        );
        apply_option_ref!(
            self,
            actions,
            applied,
            brush_settings,
            brush_settings_action_message
        );
        apply_option_ref!(
            self,
            actions,
            applied,
            load_confirm,
            load_confirm_action_message
        );
    }

    fn apply_egui_window_tail(
        &mut self,
        actions: &mut OverlayActions,
        logical: egui::Vec2,
        applied: &mut usize,
    ) {
        apply_option_ref!(
            self,
            actions,
            applied,
            audio_export,
            audio_export_action_message
        );
        apply_option_ref!(
            self,
            actions,
            applied,
            draw_settings,
            draw_settings_action_message
        );
        apply_option_ref!(
            self,
            actions,
            applied,
            memory_monitor,
            memory_monitor_action_message
        );
        apply_option_ref!(
            self,
            actions,
            applied,
            cloud_connect,
            cloud_connect_action_message
        );
        apply_option_ref!(self, actions, applied, help, help_action_message);
        apply_option_ref!(
            self,
            actions,
            applied,
            custom_precision,
            custom_precision_action_message
        );
        apply_option_ref!(
            self,
            actions,
            applied,
            speed_change,
            speed_change_action_message
        );
        apply_option_ref!(self, actions, applied, settings, settings_action_message);
        apply_option_ref!(self, actions, applied, panel, right_sidebar_action_message);
        if let Some(action) = actions.toolbar.take() {
            let message = match &action {
                lumino_ui_egui::chrome::ToolbarAction::OverflowItem(index) => {
                    self.overflow_action_message(*index, logical.x)
                }
                other => self.toolbar_action_message(other),
            };
            if let Some(message) = message {
                self.update(message);
                *applied += 1;
            }
        }
        if let Some(action) = actions.overlay_window.take() {
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
            self.update(message);
            *applied += 1;
        }
    }

    /// 形状画布动作直接改文档，不经过 `Message`（与画布侧同一逻辑）。
    fn apply_shape_canvas_actions(
        &mut self,
        actions: &[lumino_ui_egui::shape_canvas::ShapeCanvasAction],
        logical: egui::Vec2,
        applied: &mut usize,
    ) {
        if actions.is_empty() {
            return;
        }
        let panel = self.state.shape_drawer.panel_width + 6.0;
        let top = if self.settings.synth.use_native_titlebar {
            0.0
        } else {
            lumino_ui_egui::layout::TITLEBAR_H
        };
        let canvas_rect =
            egui::Rect::from_min_max(egui::pos2(panel, top), egui::pos2(logical.x, logical.y));
        for action in actions {
            self.apply_shape_canvas_action(action, canvas_rect);
            *applied += 1;
        }
    }

    fn apply_sidebar_route(&mut self, route: Option<SidebarRoute>, applied: &mut usize) {
        use crate::sidebar::Event as SidebarEvent;
        let Some(route) = route else {
            return;
        };
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
            // 底部按钮既有语义直接发 `Message`（没有 `Event` 变体）。
            SidebarRoute::RollBarHorizontal | SidebarRoute::RollBarVertical => {
                let button = match route {
                    SidebarRoute::RollBarHorizontal => {
                        lumino_ui_core::sidebar_event::RollBarButton::Horizontal
                    }
                    _ => lumino_ui_core::sidebar_event::RollBarButton::Vertical,
                };
                self.update(SidebarEvent::roll_bar_toggled(button));
                *applied += 1;
                return;
            }
            SidebarRoute::MixerToggle => {
                self.update(SidebarEvent::mixer_panel_toggled());
                *applied += 1;
                return;
            }
        };
        self.update(Message::Sidebar(event));
        *applied += 1;
    }
}
