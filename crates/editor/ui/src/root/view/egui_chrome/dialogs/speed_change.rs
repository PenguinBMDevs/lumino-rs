//! 变速对话框：生产状态 → egui 模型，以及动作 → `Message` 映射。

use lumino_core::tr;
use lumino_ui_egui::chrome::SpeedChangeModel;

use crate::Message;

impl crate::root::Root {
    /// 变速模型：标题 / 倍率行 / 提示 / 按钮。
    pub(super) fn speed_change_model(&self) -> SpeedChangeModel {
        let state = &self.state.speed_change_dialog;
        SpeedChangeModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("变速").to_string(),
            factor_label: tr!("倍率(S):").to_string(),
            factor_hint: tr!("如: 0.5 或 1/3").to_string(),
            factor: state.factor_input.clone(),
            hint: tr!("支持小数 (0.5) 或分数 (1/3)").to_string(),
            ok: tr!("确定").to_string(),
            cancel: tr!("取消").to_string(),
        }
    }

    /// egui 变速动作 → 生产 `Message`。
    pub(crate) fn speed_change_action_message(
        &self,
        action: &lumino_ui_egui::speed_change::SpeedChangeAction,
    ) -> Option<Message> {
        use crate::message::SpeedChangeAction as Prod;
        use lumino_ui_egui::speed_change::SpeedChangeAction as Action;
        Some(Message::SpeedChange(match action {
            Action::FactorChanged(text) => Prod::FactorChanged(text.clone()),
            Action::Confirm => Prod::Confirm,
            Action::Cancel => Prod::CloseDialog,
        }))
    }
}
