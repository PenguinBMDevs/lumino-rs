//! 自动保存恢复对话框（DEBT-12）：生产状态 → egui 模型，动作 → `Message`。

use lumino_core::tr;
use lumino_ui_egui::chrome::AutosaveRecoverModel;

use crate::Message;

impl crate::root::Root {
    /// 自动保存恢复模型：标题 / 路径 / 修改时间 / 说明 / 两个按钮。
    pub(super) fn autosave_recover_model(&self) -> AutosaveRecoverModel {
        let state = &self.state.autosave_recover_dialog;
        AutosaveRecoverModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("检测到未保存的自动保存").to_string(),
            path: state.path.clone(),
            time: if state.modified_at.is_empty() {
                tr!("修改时间未知").to_string()
            } else {
                format!("{}：{}", tr!("修改于"), state.modified_at)
            },
            hint: tr!("恢复不会改动任何已有文件。").to_string(),
            restore: tr!("恢复自动保存").to_string(),
            discard: tr!("不恢复").to_string(),
        }
    }

    /// egui 自动保存恢复动作 → 生产 `Message`。
    pub(crate) fn autosave_recover_action_message(
        &self,
        action: &lumino_ui_egui::autosave_recover::AutosaveRecoverAction,
    ) -> Option<Message> {
        use crate::message::AutosaveRecoverAction as Prod;
        use lumino_ui_egui::autosave_recover::AutosaveRecoverAction as Action;
        Some(Message::AutosaveRecover(match action {
            Action::Restore => Prod::Restore,
            Action::Discard => Prod::Discard,
        }))
    }
}
