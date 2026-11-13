//! 批量编辑对话框：生产状态 → egui 模型，以及动作 → `Message` 映射。

use lumino_core::tr;
use lumino_ui_egui::chrome::{BatchEditFieldModel, BatchEditModel, BatchEditRowModel};

use crate::Message;

impl crate::root::Root {
    /// 批量编辑模型：四行输入 + 格式说明 + 按钮。
    pub(super) fn batch_edit_model(&self) -> BatchEditModel {
        let state = &self.state.batch_edit_dialog;
        BatchEditModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("批量编辑").to_string(),
            settings_title: tr!("设置").to_string(),
            rows: vec![
                BatchEditRowModel {
                    field: BatchEditFieldModel::Velocity,
                    label: tr!("音符力度:").to_string(),
                    placeholder: tr!("如: +20").to_string(),
                    value: state.velocity_input.clone(),
                },
                BatchEditRowModel {
                    field: BatchEditFieldModel::Gate,
                    label: tr!("音符长度:").to_string(),
                    placeholder: tr!("如: *1.5").to_string(),
                    value: state.gate_input.clone(),
                },
                BatchEditRowModel {
                    field: BatchEditFieldModel::Key,
                    label: tr!("音符key位置:").to_string(),
                    placeholder: tr!("如: 60").to_string(),
                    value: state.key_input.clone(),
                },
                BatchEditRowModel {
                    field: BatchEditFieldModel::Tick,
                    label: tr!("音符tick位置:").to_string(),
                    placeholder: tr!("如: 1000").to_string(),
                    value: state.tick_input.clone(),
                },
            ],
            format_title: tr!("格式").to_string(),
            format_hints: vec![
                tr!("支持以下运算符号:").to_string(),
                tr!("+N  增加 N        -N  减少 N").to_string(),
                tr!("*N  乘以 N        /N  除以 N").to_string(),
                tr!("无符号数字表示直接设置为该值").to_string(),
            ],
            ok: tr!("确定").to_string(),
            cancel: tr!("取消").to_string(),
        }
    }

    /// egui 批量编辑动作 → 生产 `Message`。
    pub(crate) fn batch_edit_action_message(
        &self,
        action: &lumino_ui_egui::batch_edit::BatchEditAction,
    ) -> Option<Message> {
        use crate::message::BatchEditAction as Prod;
        use lumino_ui_egui::batch_edit::BatchEditAction as Action;
        Some(Message::BatchEdit(match action {
            Action::InputChanged(field, text) => {
                Prod::InputChanged(batch_edit_field(*field), text.clone())
            }
            Action::Confirm => Prod::Confirm,
            Action::Cancel => Prod::CloseDialog,
        }))
    }
}

/// 模型字段 → 生产字段。
fn batch_edit_field(field: BatchEditFieldModel) -> crate::message::BatchEditField {
    use crate::message::BatchEditField as Prod;
    match field {
        BatchEditFieldModel::Velocity => Prod::Velocity,
        BatchEditFieldModel::Gate => Prod::Gate,
        BatchEditFieldModel::Key => Prod::Key,
        BatchEditFieldModel::Tick => Prod::Tick,
    }
}
