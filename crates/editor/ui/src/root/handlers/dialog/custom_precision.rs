//! 自定义精度对话框处理器

use crate::host::DialogResult;
use crate::message::{CustomPrecisionAction, Message};
use crate::root::Root;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_custom_precision(
        &self,
        root: &mut Root,
        action: CustomPrecisionAction,
    ) -> Option<Message> {
        match action {
            CustomPrecisionAction::OpenDialog => {
                self.handle_custom_precision_dialog_open(root);
            }
            CustomPrecisionAction::CloseDialog => {
                self.handle_custom_precision_dialog_close(root);
            }
            CustomPrecisionAction::Confirm => {
                self.handle_confirm_custom_precision(root);
            }
            CustomPrecisionAction::TupletCountChanged(value) => {
                Self::update_precision_if_digit(
                    &mut root.state.custom_precision_dialog.tuplet_count,
                    &value,
                );
            }
            CustomPrecisionAction::TupletTypeChanged(value) => {
                root.state.custom_precision_dialog.tuplet_type = value;
                root.state.custom_precision_dialog.tuplet_count = value.value().to_string();
            }
            CustomPrecisionAction::DotTypeChanged(value) => {
                root.state.custom_precision_dialog.dot_type = value;
            }
            CustomPrecisionAction::NoteValueChanged(value) => {
                Self::update_precision_if_digit(
                    &mut root.state.custom_precision_dialog.note_value,
                    &value,
                );
            }
            CustomPrecisionAction::DivisorChanged(value) => {
                Self::update_precision_if_digit(
                    &mut root.state.custom_precision_dialog.divisor,
                    &value,
                );
            }
        }
        None
    }

    fn handle_custom_precision_dialog_open(&self, _root: &mut Root) {
        tracing::info!("Root: 请求打开自定义精度对话框");
        crate::event::emit(crate::event::Event::Window(
            crate::event::window::Event::open_custom_precision_dialog(),
        ));
    }

    fn handle_custom_precision_dialog_close(&self, root: &mut Root) {
        root.state.custom_precision_dialog.is_open = false;
        root.state.dialog_result = Some(DialogResult::Cancel);
    }

    fn handle_confirm_custom_precision(&self, root: &mut Root) {
        let dialog = &root.state.custom_precision_dialog;

        if dialog.calculate_ticks(1).is_none() {
            tracing::warn!("自定义精度: 无效的输入值");
            return;
        }

        // 设置对话框结果，由 runner 在主窗口应用精度
        let denominator = match (
            dialog.note_value.parse::<f32>(),
            dialog.divisor.parse::<f32>(),
        ) {
            (Ok(nv), Ok(div)) if nv > 0.0 && div > 0.0 => (nv * div).to_string(),
            _ => {
                tracing::warn!("自定义精度: 无法解析 note_value/divisor");
                return;
            }
        };

        root.state.dialog_result = Some(DialogResult::CustomPrecision {
            numerator: dialog.tuplet_count.clone(),
            denominator,
        });
        root.state.custom_precision_dialog.is_open = false;
        tracing::info!("自定义精度已提交，等待应用");
    }

    fn update_precision_if_digit(target: &mut String, value: &str) {
        if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
            *target = value.to_string();
        }
    }
}
