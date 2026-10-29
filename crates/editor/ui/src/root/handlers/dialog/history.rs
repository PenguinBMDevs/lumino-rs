//! 撤销历史窗口处理器（DRAW-035）

use crate::host::DialogResult;
use crate::message::HistoryAction;
use crate::root::Root;

impl Root {
    /// 处理历史面板动作；跳转由 Runner 调主窗口 Editor 执行。
    pub(crate) fn handle_history_action(&mut self, action: HistoryAction) {
        match action {
            HistoryAction::Jump(index) => {
                self.state.dialog_result = Some(DialogResult::HistoryJump(index));
            }
            HistoryAction::Close => {
                self.state.dialog_result = Some(DialogResult::Cancel);
            }
        }
    }
}
