//! 保存确认对话框结果处理
//!
//! 关闭工程 / 打开另一个工程 / 退出软件前，若工程存在未保存更改，
//! 弹出的确认对话框。三种结果：
//! - 保存：保存当前工程后由 Runner 继续原操作
//! - 关闭：放弃未保存更改，直接关闭（不保存）
//! - 取消：取消关闭操作，继续编辑

use crate::root::Root;

impl Root {
    /// 保存确认对话框 - 保存
    pub(crate) fn handle_save_confirm_save(&mut self) {
        self.state.dialog_result = Some(crate::host::DialogResult::SaveConfirm(
            crate::message::SaveConfirmAction::Save,
        ));
        self.state.save_confirm_dialog.is_open = false;
    }

    /// 保存确认对话框 - 关闭（放弃更改）
    pub(crate) fn handle_save_confirm_discard(&mut self) {
        self.state.dialog_result = Some(crate::host::DialogResult::SaveConfirm(
            crate::message::SaveConfirmAction::Discard,
        ));
        self.state.save_confirm_dialog.is_open = false;
    }

    /// 保存确认对话框 - 取消
    pub(crate) fn handle_save_confirm_cancel(&mut self) {
        self.state.dialog_result = Some(crate::host::DialogResult::SaveConfirm(
            crate::message::SaveConfirmAction::Cancel,
        ));
        self.state.save_confirm_dialog.is_open = false;
    }
}
