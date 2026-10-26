//! 自定义精度对话框处理器

use lumino_core::shape::expr::{eval_int, is_int_input};

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
                Self::update_precision_input(
                    &mut root.state.custom_precision_dialog.tuplet_count,
                    &value,
                    &mut root.state.custom_precision_dialog.error,
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
                Self::update_precision_input(
                    &mut root.state.custom_precision_dialog.note_value,
                    &value,
                    &mut root.state.custom_precision_dialog.error,
                );
            }
            CustomPrecisionAction::DivisorChanged(value) => {
                Self::update_precision_input(
                    &mut root.state.custom_precision_dialog.divisor,
                    &value,
                    &mut root.state.custom_precision_dialog.error,
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
        let dialog = &mut root.state.custom_precision_dialog;

        let numerator = match eval_int(&dialog.tuplet_count) {
            Ok(value) if value > 0 => value,
            Ok(_) => {
                dialog.error = Some("三连音数必须大于 0".to_string());
                return;
            }
            Err(error) => {
                dialog.error = Some(format!("三连音数：{error}"));
                return;
            }
        };
        let note_value = match eval_int(&dialog.note_value) {
            Ok(value) if value > 0 => value,
            Ok(_) => {
                dialog.error = Some("分音符必须大于 0".to_string());
                return;
            }
            Err(error) => {
                dialog.error = Some(format!("分音符：{error}"));
                return;
            }
        };
        let divisor = match eval_int(&dialog.divisor) {
            Ok(value) if value > 0 => value,
            Ok(_) => {
                dialog.error = Some("除数必须大于 0".to_string());
                return;
            }
            Err(error) => {
                dialog.error = Some(format!("除数：{error}"));
                return;
            }
        };

        // 设置对话框结果，由 runner 在主窗口应用精度。
        // runner 按 f32 解析这两个字段，因此这里必须写入**求值后的数字**，
        // 不能把 `960*4` 这种表达式原样传过去。
        dialog.error = None;
        root.state.dialog_result = Some(DialogResult::CustomPrecision {
            numerator: numerator.to_string(),
            denominator: (note_value * divisor).to_string(),
        });
        root.state.custom_precision_dialog.is_open = false;
        tracing::info!("自定义精度已提交，等待应用");
    }

    fn update_precision_input(target: &mut String, value: &str, error: &mut Option<String>) {
        if is_int_input(value) {
            *target = value.to_string();
            *error = None;
        }
    }
}
