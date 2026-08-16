//! 批量编辑对话框处理器

use crate::host::DialogResult;
use crate::message::{BatchEditAction, BatchEditField, Message};
use crate::root::Root;
use lumino_ui_core::state::parse_batch_edit_input;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_batch_edit(
        &self,
        root: &mut Root,
        action: BatchEditAction,
    ) -> Option<Message> {
        match action {
            BatchEditAction::OpenDialog => {
                self.handle_batch_edit_dialog_open(root);
            }
            BatchEditAction::CloseDialog => {
                self.handle_batch_edit_dialog_close(root);
            }
            BatchEditAction::Confirm => {
                self.handle_confirm_batch_edit(root);
            }
            BatchEditAction::InputChanged(field, value) => {
                self.handle_batch_edit_input_changed(root, field, value);
            }
        }
        None
    }

    fn handle_batch_edit_dialog_open(&self, _root: &mut Root) {
        tracing::info!("Root: 请求打开批量编辑对话框");
        crate::event::emit(crate::event::Event::Window(
            crate::event::window::Event::open_batch_edit_dialog(),
        ));
    }

    fn handle_batch_edit_dialog_close(&self, root: &mut Root) {
        root.state.batch_edit_dialog.is_open = false;
        root.state.dialog_result = Some(DialogResult::Cancel);
    }

    fn handle_confirm_batch_edit(&self, root: &mut Root) {
        let dialog = &root.state.batch_edit_dialog;

        // 允许全部为空（表示无操作），但至少需要有合法输入才设置结果
        let velocity = dialog.velocity_input.trim().to_string();
        let gate = dialog.gate_input.trim().to_string();
        let key = dialog.key_input.trim().to_string();
        let tick = dialog.tick_input.trim().to_string();

        let has_operation =
            !velocity.is_empty() || !gate.is_empty() || !key.is_empty() || !tick.is_empty();

        if has_operation && !velocity.is_empty() && parse_batch_edit_input(&velocity).is_none() {
            tracing::warn!("批量编辑: 音符力度输入无效: {}", velocity);
            return;
        }
        if has_operation && !gate.is_empty() && parse_batch_edit_input(&gate).is_none() {
            tracing::warn!("批量编辑: 音符长度输入无效: {}", gate);
            return;
        }
        if has_operation && !key.is_empty() && parse_batch_edit_input(&key).is_none() {
            tracing::warn!("批量编辑: 音符key位置输入无效: {}", key);
            return;
        }
        if has_operation && !tick.is_empty() && parse_batch_edit_input(&tick).is_none() {
            tracing::warn!("批量编辑: 音符tick位置输入无效: {}", tick);
            return;
        }

        root.state.dialog_result = Some(DialogResult::BatchEdit {
            velocity,
            gate,
            key,
            tick,
        });
        root.state.batch_edit_dialog.is_open = false;
        tracing::info!("批量编辑已提交，等待应用");
    }

    fn handle_batch_edit_input_changed(
        &self,
        root: &mut Root,
        field: BatchEditField,
        value: String,
    ) {
        let dialog = &mut root.state.batch_edit_dialog;
        match field {
            BatchEditField::Velocity => dialog.velocity_input = value,
            BatchEditField::Gate => dialog.gate_input = value,
            BatchEditField::Key => dialog.key_input = value,
            BatchEditField::Tick => dialog.tick_input = value,
        }
    }
}
