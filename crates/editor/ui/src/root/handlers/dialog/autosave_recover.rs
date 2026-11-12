//! 自动保存恢复对话框处理器（DEBT-12）

use crate::message::{AutosaveRecoverAction, Message};
use crate::root::Root;

use super::DialogHandler;

impl DialogHandler {
    /// 处理自动保存恢复对话框动作
    pub(super) fn handle_autosave_recover(
        &self,
        root: &mut Root,
        action: AutosaveRecoverAction,
    ) -> Option<Message> {
        root.handle_autosave_recover(action);
        None
    }
}
