//! Runner 编辑菜单事件处理

use crate::runner::RunnerInner;

impl RunnerInner {
    /// 处理编辑菜单事件
    pub(super) fn handle_edit_menu_event(&mut self, event: lumino_core::event::menu::edit::Event) {
        use lumino_core::event::menu::edit::Event::*;

        match event {
            Undo => {
                tracing::info!("Runner: 处理撤销操作");
                // 发送撤销消息到UI
                let ui = self.window.ui_mut();
                ui.handle_action(lumino_ui::message::EditorAction::Undo);
            }
            Redo => {
                tracing::info!("Runner: 处理重做操作");
                // 发送重做消息到UI
                let ui = self.window.ui_mut();
                ui.handle_action(lumino_ui::message::EditorAction::Redo);
            }
            _ => {
                tracing::debug!("Runner: 编辑事件 {:?} 未实现", event);
            }
        }
    }
}
