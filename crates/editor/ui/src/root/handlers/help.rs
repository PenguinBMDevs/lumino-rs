//! 帮助系统处理器（DRAW-038）
//!
//! Help 窗口是独立 Root，但搜索/选中仍是本地状态；首次 tip 只由主 Root 触发。
//! 持久化统一交给 Runner（`HelpTipSeen` / `ResetHelpTips` 窗口事件）。

use lumino_message::{HelpAction, Tool};

use crate::root::Root;

impl Root {
    /// 处理 Help 窗口/首次提示动作。
    pub(crate) fn handle_help_action(&mut self, action: HelpAction) {
        match action {
            HelpAction::QueryChanged(query) => {
                self.state.help_dialog.query = query;
                self.state.help_dialog.selected = 0;
            }
            HelpAction::TopicSelected(index) => {
                self.state.help_dialog.selected = index;
            }
            HelpAction::DismissTip => self.state.help_tips.dismiss(),
            HelpAction::ResetTips => {
                self.state.help_tips.reset();
                self.emit_dialog_event(crate::event::window::Event::reset_help_tips());
            }
            HelpAction::Close => {
                self.emit_dialog_event(crate::event::window::Event::close_help_dialog());
            }
        }
    }

    /// 工具首次选中时弹一次 tip（已看过则不弹）。
    pub(crate) fn maybe_show_help_tip(&mut self, tool: Tool) {
        if self.state.is_dialog_window {
            return;
        }
        let Some(topic) = lumino_ui_core::help::topic_for_tool(tool) else {
            return;
        };
        if self.state.help_tips.should_show(topic.id) && self.state.help_tips.show(topic.id) {
            self.emit_dialog_event(crate::event::window::Event::help_tip_seen(
                topic.id.to_string(),
            ));
        }
    }

    /// 「全部重现」：清空内存集合（Runner 会同时清配置）。
    pub fn reset_help_tips(&mut self) {
        self.state.help_tips.reset();
    }

    fn emit_dialog_event(&self, event: crate::event::window::Event) {
        crate::event::emit(crate::event::Event::Window(event));
    }
}
