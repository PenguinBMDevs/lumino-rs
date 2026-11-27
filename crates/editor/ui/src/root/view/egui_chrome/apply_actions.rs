//! egui 动作消费：生产 `Host` 与原生运行时共用同一份翻译逻辑。
//!
//! 通道表的唯一来源是 [`OverlayActions`]。本文件把动作翻译成「有序消息 + 直接状态变更」
//! （[`EguiPlan`]），再由调用方决定消息怎么落地：
//! `Host` 走 `process_message`（窗口 / 主题 / 编辑器路由），原生运行时走 `Root::update`。
//! 新增一个通道只需改 `OverlayActions` 与本文件的 plan 两处，不再维护第二份 dispatch。

use lumino_ui_egui::chrome::sidebar::SidebarRoute;
use lumino_ui_egui::scrollbar::ScrollbarAction;

use crate::Message;
use crate::host::OverlayActions;
use crate::root::Root;
use crate::state::root_state::DialogType;

/// 恒返回 `Message` 的通道。
macro_rules! push_always {
    ($self:ident, $actions:ident, $plan:ident, $field:ident, $mapper:ident) => {{
        if let Some(action) = $actions.$field.take() {
            $plan.messages.push($self.$mapper(action));
        }
    }};
}

/// 返回 `Option<Message>` 的通道。
macro_rules! push_option {
    ($self:ident, $actions:ident, $plan:ident, $field:ident, $mapper:ident) => {{
        if let Some(action) = $actions.$field.take()
            && let Some(message) = $self.$mapper(action)
        {
            $plan.messages.push(message);
        }
    }};
}

/// 同 [`push_option`]，映射器借用动作。
macro_rules! push_option_ref {
    ($self:ident, $actions:ident, $plan:ident, $field:ident, $mapper:ident) => {{
        if let Some(action) = $actions.$field.take()
            && let Some(message) = $self.$mapper(&action)
        {
            $plan.messages.push(message);
        }
    }};
}

/// 一帧 egui 动作的应用计划。
///
/// `messages` 按原 dispatch 顺序排列；其余字段需要 Root 直接改状态，
/// 不经过 `Message`（Toast 关闭、混音台对话框、形状画布）。
pub struct EguiPlan {
    /// 待应用的消息（顺序即语义）。
    pub messages: Vec<Message>,
    /// Toast 关闭 id。
    pub toast_dismiss: Option<u64>,
    /// 混音台对话框内动作。
    pub mixer_dialog: Vec<lumino_ui_egui::mixer::MixerAction>,
    /// 形状画布动作（共享同一个画布矩形）。
    pub shape_canvas: Vec<lumino_ui_egui::shape_canvas::ShapeCanvasAction>,
    /// 本帧形状画布矩形（由视口逻辑尺寸与侧栏宽度算出）。
    pub canvas_rect: egui::Rect,
}

impl Root {
    /// 把本帧动作翻译成计划，不改任何状态。
    pub fn plan_egui_actions(&self, actions: OverlayActions, logical: egui::Vec2) -> EguiPlan {
        let mut actions = actions;
        let mut plan = EguiPlan {
            messages: Vec::new(),
            toast_dismiss: None,
            mixer_dialog: Vec::new(),
            shape_canvas: Vec::new(),
            canvas_rect: shape_canvas_rect(self, logical),
        };
        self.plan_egui_blurs(&mut actions, &mut plan);
        self.plan_egui_before_canvas(actions, logical, &mut plan);
        plan
    }

    /// 应用本帧全部 egui 动作，返回生效条数（消息 + 直接变更）。
    pub fn apply_egui_actions(&mut self, actions: OverlayActions, logical: egui::Vec2) -> usize {
        let mut plan = self.plan_egui_actions(actions, logical);
        let messages = std::mem::take(&mut plan.messages);
        let applied = messages.len();
        for message in messages {
            self.update(message);
        }
        applied + self.apply_egui_mutations(&mut plan)
    }

    /// 应用计划里的直接状态变更；返回条数。
    ///
    /// `Host` 也用这个入口，避免「消息走 Host、变更走 Root」的隐性分叉。
    pub fn apply_egui_mutations(&mut self, plan: &mut EguiPlan) -> usize {
        let mut applied = 0;
        if let Some(id) = plan.toast_dismiss.take() {
            self.toast.dismiss(id);
            applied += 1;
        }
        for action in plan.mixer_dialog.drain(..) {
            self.apply_mixer_dialog_action(action);
            applied += 1;
        }
        let rect = plan.canvas_rect;
        for action in plan.shape_canvas.drain(..) {
            self.apply_shape_canvas_action(&action, rect);
            applied += 1;
        }
        applied
    }

    /// 输入框失焦与通知：必须抢在其它动作之前收口。
    fn plan_egui_blurs(&self, actions: &mut OverlayActions, plan: &mut EguiPlan) {
        if let Some(blur) = actions.dashboard_blur.take() {
            plan.messages
                .push(plan_dashboard_blur(self, blur.field, blur.outcome));
        }
        if let Some(blur) = actions.sidebar_rename_blur.take() {
            plan.messages
                .extend(self.sidebar_rename_blur_messages(blur));
        }
        if let Some(lumino_ui_egui::chrome::toast::ToastAction::Dismiss(id)) = actions.toast.take()
        {
            plan.toast_dismiss = Some(id);
        }
    }

    /// 画布动作之前的通道（工具栏 / 菜单 / 对话框）。
    fn plan_egui_before_canvas(
        &self,
        mut actions: OverlayActions,
        logical: egui::Vec2,
        plan: &mut EguiPlan,
    ) {
        push_always!(self, actions, plan, titlebar, titlebar_action_message);
        push_always!(
            self,
            actions,
            plan,
            first_screen,
            first_screen_action_message
        );
        push_always!(
            self,
            actions,
            plan,
            piano_roll_menu,
            piano_roll_context_menu_message
        );
        push_always!(self, actions, plan, editor_buttons, editor_button_message);
        push_always!(self, actions, plan, bend_buttons, bend_button_message);
        push_always!(self, actions, plan, text_tool, text_tool_message);
        push_always!(self, actions, plan, velocity_panel, velocity_panel_message);
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
            plan.messages.push(message);
        }
        push_option!(self, actions, plan, menu, menu_action_message);
        push_option_ref!(self, actions, plan, dialog, save_confirm_action_message);
        push_option_ref!(
            self,
            actions,
            plan,
            project,
            project_settings_action_message
        );
        push_option_ref!(
            self,
            actions,
            plan,
            port_soundfonts,
            port_soundfonts_action_message
        );
        push_option_ref!(
            self,
            actions,
            plan,
            port_soundfonts_unsupported,
            port_soundfonts_unsupported_action_message
        );
        push_option_ref!(
            self,
            actions,
            plan,
            autosave,
            autosave_recover_action_message
        );
        push_option_ref!(self, actions, plan, history, history_action_message);
        push_option_ref!(self, actions, plan, batch_edit, batch_edit_action_message);
        plan.shape_canvas = std::mem::take(&mut actions.shape_canvas);
        self.plan_egui_after_canvas(&mut actions, logical, plan);
    }

    /// 画布动作之后的通道（面板 / 列表 / 独立对话框）。
    fn plan_egui_after_canvas(
        &self,
        actions: &mut OverlayActions,
        logical: egui::Vec2,
        plan: &mut EguiPlan,
    ) {
        push_always!(self, actions, plan, help_tip, help_tip_action_message);
        push_option!(
            self,
            actions,
            plan,
            sidebar_menu,
            sidebar_menu_action_message
        );
        push_always!(
            self,
            actions,
            plan,
            sidebar_panel,
            sidebar_panel_action_message
        );
        push_always!(self, actions, plan, statusbar, statusbar_action_message);
        push_always!(self, actions, plan, mask_panel, mask_panel_action_message);
        push_always!(
            self,
            actions,
            plan,
            material_delete,
            material_delete_action_message
        );
        push_always!(
            self,
            actions,
            plan,
            tempo_max_bpm,
            tempo_max_bpm_action_message
        );
        for action in std::mem::take(&mut actions.mixer) {
            if self.state.dialog_type == DialogType::Mixer {
                plan.mixer_dialog.push(action);
            } else if let Some(message) = self.mixer_action_message(&action) {
                plan.messages.push(message);
            }
        }
        for action in std::mem::take(&mut actions.video_timeline) {
            let message =
                self.video_timeline_action_message(&action, self.state.video_clip.preview_width);
            plan.messages.push(message);
        }
        for action in std::mem::take(&mut actions.lane) {
            let message = self.lane_action_message(&action);
            plan.messages.push(message);
        }
        push_option_ref!(self, actions, plan, arrangement, arrangement_action_message);
        push_option_ref!(
            self,
            actions,
            plan,
            video_overlay,
            video_overlay_action_message
        );
        plan_sidebar_route(actions.sidebar_route.take(), &mut plan.messages);
        push_option_ref!(
            self,
            actions,
            plan,
            video_export,
            video_export_action_message
        );
        push_option_ref!(
            self,
            actions,
            plan,
            shape_drawer,
            shape_drawer_action_message
        );
        push_option_ref!(
            self,
            actions,
            plan,
            cloud_browser,
            cloud_browser_action_message
        );
        push_option_ref!(
            self,
            actions,
            plan,
            collaboration,
            collaboration_action_message
        );
        push_option_ref!(
            self,
            actions,
            plan,
            recover_track,
            recover_track_action_message
        );
        push_option_ref!(
            self,
            actions,
            plan,
            brush_settings,
            brush_settings_action_message
        );
        push_option_ref!(
            self,
            actions,
            plan,
            load_confirm,
            load_confirm_action_message
        );
        push_option_ref!(
            self,
            actions,
            plan,
            audio_export,
            audio_export_action_message
        );
        push_option_ref!(
            self,
            actions,
            plan,
            draw_settings,
            draw_settings_action_message
        );
        push_option_ref!(
            self,
            actions,
            plan,
            memory_monitor,
            memory_monitor_action_message
        );
        push_option_ref!(
            self,
            actions,
            plan,
            cloud_connect,
            cloud_connect_action_message
        );
        push_option_ref!(self, actions, plan, help, help_action_message);
        push_option_ref!(
            self,
            actions,
            plan,
            custom_precision,
            custom_precision_action_message
        );
        push_option_ref!(
            self,
            actions,
            plan,
            speed_change,
            speed_change_action_message
        );
        push_option_ref!(self, actions, plan, settings, settings_action_message);
        push_option_ref!(self, actions, plan, panel, right_sidebar_action_message);
        if let Some(action) = actions.toolbar.take() {
            let message = match &action {
                lumino_ui_egui::chrome::ToolbarAction::OverflowItem(index) => {
                    self.overflow_action_message(*index, logical.x)
                }
                other => self.toolbar_action_message(other),
            };
            if let Some(message) = message {
                plan.messages.push(message);
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
            plan.messages.push(message);
        }
    }
}

fn plan_dashboard_blur(
    root: &Root,
    field: lumino_ui_egui::chrome::toolbar::DashboardField,
    outcome: lumino_ui_egui::chrome::toolbar::MetricBlur,
) -> Message {
    use lumino_ui_egui::chrome::toolbar::{DashboardField, MetricBlur};
    match (field, outcome) {
        (DashboardField::Bpm, MetricBlur::Commit) => crate::toolbar::Event::bpm_edit_confirmed(),
        (DashboardField::Bpm, MetricBlur::Cancel) => {
            crate::toolbar::Event::bpm_edit_toggled(root.dashboard_bpm())
        }
        (DashboardField::Ppq, MetricBlur::Commit) => crate::toolbar::Event::ppq_edit_confirmed(),
        (DashboardField::Ppq, MetricBlur::Cancel) => {
            crate::toolbar::Event::ppq_edit_toggled(root.editor.editor_state.view.ppq)
        }
    }
}

/// 形状画布矩形：左侧让出形状抽屉与分隔条，顶部让出标题栏。
fn shape_canvas_rect(root: &Root, logical: egui::Vec2) -> egui::Rect {
    let panel = root.state.shape_drawer.panel_width + 6.0;
    let top = if root.settings.synth.use_native_titlebar {
        0.0
    } else {
        lumino_ui_egui::layout::TITLEBAR_H
    };
    egui::Rect::from_min_max(egui::pos2(panel, top), egui::pos2(logical.x, logical.y))
}

fn plan_sidebar_route(route: Option<SidebarRoute>, messages: &mut Vec<Message>) {
    use crate::sidebar::Event as SidebarEvent;
    let Some(route) = route else {
        return;
    };
    let event = match route {
        SidebarRoute::PianoRollGroup => {
            SidebarEvent::GroupToggled(crate::sidebar::GroupId::PianoRoll)
        }
        SidebarRoute::ProjectGroup => SidebarEvent::GroupToggled(crate::sidebar::GroupId::Project),
        SidebarRoute::WaterfallGroup => {
            SidebarEvent::GroupToggled(crate::sidebar::GroupId::Waterfall)
        }
        SidebarRoute::RendererGroup => {
            SidebarEvent::GroupToggled(crate::sidebar::GroupId::Renderer)
        }
        SidebarRoute::FilePanel => SidebarEvent::PanelToggled(crate::sidebar::Route::File),
        SidebarRoute::AutomationPanel => SidebarEvent::AutomationPanelToggled,
        SidebarRoute::VideoExport => SidebarEvent::RouteUpdated(crate::sidebar::Route::VideoExport),
        SidebarRoute::AudioExport => SidebarEvent::RouteUpdated(crate::sidebar::Route::AudioExport),
        SidebarRoute::RollBarHorizontal | SidebarRoute::RollBarVertical => {
            let button = match route {
                SidebarRoute::RollBarHorizontal => {
                    lumino_ui_core::sidebar_event::RollBarButton::Horizontal
                }
                _ => lumino_ui_core::sidebar_event::RollBarButton::Vertical,
            };
            messages.push(SidebarEvent::roll_bar_toggled(button));
            return;
        }
        SidebarRoute::MixerToggle => {
            messages.push(SidebarEvent::mixer_panel_toggled());
            return;
        }
    };
    messages.push(Message::Sidebar(event));
}

#[cfg(test)]
mod tests {
    use super::*;
    use lumino_ui_egui::chrome::toast::ToastAction;
    use lumino_ui_egui::scrollbar::{DirectedAction, ScrollbarAction};

    fn root() -> Root {
        Root::new(&lumino_core::storage::config::UiConfig::default())
    }

    /// 空动作表不产生任何消息或状态变更。
    #[test]
    fn empty_actions_produce_empty_plan() {
        let root = root();
        let plan = root.plan_egui_actions(OverlayActions::default(), egui::vec2(800.0, 600.0));
        assert!(plan.messages.is_empty(), "空动作不应产生消息");
        assert!(plan.toast_dismiss.is_none(), "空动作不应关 Toast");
        assert!(plan.mixer_dialog.is_empty(), "空动作不应有混音台动作");
        assert!(plan.shape_canvas.is_empty(), "空动作不应有画布动作");
    }

    /// Toast 关闭走直接变更，不伪装成 Message。
    #[test]
    fn toast_dismiss_is_a_mutation() {
        let root = root();
        let actions = OverlayActions {
            toast: Some(ToastAction::Dismiss(7)),
            ..Default::default()
        };
        let plan = root.plan_egui_actions(actions, egui::vec2(800.0, 600.0));
        assert!(plan.messages.is_empty(), "Toast 关闭不应产生消息");
        assert_eq!(plan.toast_dismiss, Some(7));
    }

    /// 滚动条动作按通道映射成对应消息（顺序与方向都保留）。
    #[test]
    fn scrollbar_maps_to_directed_message() {
        let root = root();
        let actions = OverlayActions {
            scrollbar: Some(DirectedAction {
                vertical: true,
                action: ScrollbarAction::Scroll(12.0),
            }),
            ..Default::default()
        };
        let plan = root.plan_egui_actions(actions, egui::vec2(800.0, 600.0));
        assert!(
            matches!(
                plan.messages.as_slice(),
                [Message::ScrollbarScrolledY(12.0)]
            ),
            "垂直滚动应映射为 ScrollbarScrolledY，实际：{:?}",
            plan.messages
        );
    }

    /// 生产 Host 不得再维护第二份 `*_action_message` 映射表。
    #[test]
    fn host_has_no_second_dispatch_table() {
        let src = include_str!("../../../host/render/egui_overlay/actions.rs");
        assert!(
            !src.contains("_action_message("),
            "Host 侧不允许再出现 action_message 映射，统一走 Root::plan_egui_actions"
        );
        assert!(
            !src.contains("dispatch_action_always"),
            "Host 侧的 dispatch_action_always 必须删除，避免第二份通道表"
        );
    }

    /// 通道覆盖：`OverlayActions` 的每个字段都必须在本文件的 plan 里被消费。
    ///
    /// 没有这条守卫时，新增通道只改 `OverlayActions` 也能编译通过——动作被
    /// 静默丢弃，直到用户点按钮没反应才发现。
    #[test]
    fn every_overlay_channel_is_consumed() {
        const ACTIONS_SRC: &str = include_str!("../../../host/render/egui_overlay/actions.rs");
        const PLAN_SRC: &str = include_str!("apply_actions.rs");

        let body_start = ACTIONS_SRC
            .find("pub struct OverlayActions {")
            .expect("OverlayActions 结构体必须存在");
        let body = &ACTIONS_SRC[body_start..];
        let body_end = body.find("\n}").expect("OverlayActions 结构体必须有闭合");
        let fields: Vec<&str> = body[..body_end]
            .lines()
            .filter_map(|line| line.trim().strip_prefix("pub "))
            .filter_map(|rest| rest.split_once(':').map(|(name, _)| name))
            .collect();
        assert_eq!(
            fields.len(),
            51,
            "通道数变化时请同步更新本断言，并确认新通道已接入 plan"
        );

        let normalized = PLAN_SRC.split_whitespace().collect::<Vec<_>>().join(" ");
        let missing: Vec<_> = fields
            .into_iter()
            .filter(|field| {
                !normalized.contains(&format!(", {field},"))
                    && !normalized.contains(&format!("actions.{field}"))
            })
            .collect();
        assert!(
            missing.is_empty(),
            "OverlayActions 有 {} 个通道没有消费者（动作会被静默丢弃）：{missing:?}",
            missing.len()
        );
    }
}
