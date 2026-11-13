//! 载入确认对话框：生产状态 → egui 模型，以及动作 → `Message` 映射。

use lumino_core::{tr, trf};
use lumino_ui_egui::chrome::LoadConfirmModel;

use crate::Message;

impl crate::root::Root {
    /// 载入确认模型：文件名 / 大小 / 路径 / 提示 / 两个按钮。
    pub(super) fn load_confirm_model(&self) -> LoadConfirmModel {
        let state = &self.state.load_confirm_dialog;
        LoadConfirmModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            file_line: trf!("文件: {name}", name = state.file_name),
            size_line: trf!("大小: {size} MB", size = format!("{:.1}", state.size_mb)),
            path_line: trf!("路径: {path}", path = state.file_path),
            hint: tr!("此文件较大，加载可能需要一些时间…").to_string(),
            cancel: tr!("取消").to_string(),
            confirm: tr!("加载").to_string(),
        }
    }

    /// egui 载入确认动作 → 生产 `Message`。
    pub(crate) fn load_confirm_action_message(
        &self,
        action: &lumino_ui_egui::load_confirm::LoadConfirmAction,
    ) -> Option<Message> {
        use crate::message::LoadConfirmAction as Prod;
        use lumino_ui_egui::load_confirm::LoadConfirmAction as Action;
        Some(Message::LoadConfirm(match action {
            Action::Confirm => Prod::Confirm,
            Action::Cancel => Prod::CloseDialog,
        }))
    }
}
