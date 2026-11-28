//! 输入变更类对话框事件：首次提示状态的记录与重置。

use crate::runner::RunnerInner;

impl RunnerInner {
    pub(super) fn on_help_tip_seen(&mut self, id: String) {
        self.window_state.storage.config.patch(|config| {
            if !config.ui.seen_help_tips.iter().any(|seen| seen == &id) {
                config.ui.seen_help_tips.push(id);
            }
        });
        if let Err(error) = self.window_state.storage.config.save() {
            tracing::warn!("保存帮助首次提示状态失败：{error}");
        }
    }

    pub(super) fn on_reset_help_tips(&mut self) {
        self.window_state
            .storage
            .config
            .patch(|config| config.ui.seen_help_tips.clear());
        if let Err(error) = self.window_state.storage.config.save() {
            tracing::warn!("清除帮助首次提示状态失败：{error}");
        }
        self.window_state
            .window
            .ui_mut()
            .root_mut()
            .reset_help_tips();
    }
}
