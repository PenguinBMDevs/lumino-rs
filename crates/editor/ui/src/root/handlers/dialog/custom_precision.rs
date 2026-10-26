//! 自定义精度对话框处理器

use lumino_core::shape::expr::is_int_input;

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
        match dialog.snap_spec() {
            Ok(spec) => {
                dialog.error = None;
                root.state.dialog_result = Some(DialogResult::CustomPrecision(spec));
                root.state.custom_precision_dialog.is_open = false;
                tracing::info!("自定义吸附已提交：{:?}，等待应用", spec);
            }
            Err(error) => {
                tracing::warn!("自定义吸附输入非法：{error}");
                dialog.error = Some(error);
            }
        }
    }

    fn update_precision_input(target: &mut String, value: &str, error: &mut Option<String>) {
        if is_int_input(value) {
            *target = value.to_string();
            *error = None;
        }
    }
}
