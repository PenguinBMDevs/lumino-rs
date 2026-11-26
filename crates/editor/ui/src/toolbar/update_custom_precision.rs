//! 自定义精度对话框的状态机。

use lumino_core::shape::expr::is_int_input as is_expression_input;

use super::*;

impl Toolbar {
    pub(super) fn update_custom_precision(&mut self, event: Event) {
        match event {
            Event::OpenCustomPrecisionDialog => {
                self.custom_precision_dialog.is_open = true;
                tracing::debug!("工具栏: 打开自定义精度对话框");
            }
            Event::CloseCustomPrecisionDialog => {
                self.custom_precision_dialog.is_open = false;
                tracing::debug!("工具栏: 关闭自定义精度对话框");
            }
            Event::ConfirmCustomPrecision => {
                self.custom_precision_dialog.is_open = false;
                tracing::debug!("工具栏: 确认自定义精度");
            }
            Event::CustomPrecisionTupletCountChanged(value) if is_expression_input(&value) => {
                self.custom_precision_dialog.tuplet_count = value;
                self.custom_precision_dialog.error = None;
            }
            Event::CustomPrecisionTupletTypeChanged(value) => {
                self.custom_precision_dialog.tuplet_type = value;
                self.custom_precision_dialog.tuplet_count = value.value().to_string();
            }
            Event::CustomPrecisionDotTypeChanged(value) => {
                self.custom_precision_dialog.dot_type = value;
            }
            Event::CustomPrecisionNoteValueChanged(value) if is_expression_input(&value) => {
                self.custom_precision_dialog.note_value = value;
                self.custom_precision_dialog.error = None;
            }
            Event::CustomPrecisionDivisorChanged(value) if is_expression_input(&value) => {
                self.custom_precision_dialog.divisor = value;
                self.custom_precision_dialog.error = None;
            }
            _ => {}
        }
    }
}
