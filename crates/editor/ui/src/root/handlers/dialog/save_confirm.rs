//! 保存确认对话框处理器

use crate::message::{Message, SaveConfirmAction};
use crate::root::Root;

use super::DialogHandler;

impl DialogHandler {
    /// 处理保存确认对话框动作
    pub(super) fn handle_save_confirm(
        &self,
        root: &mut Root,
        action: SaveConfirmAction,
    ) -> Option<Message> {
        match action {
            SaveConfirmAction::Save => root.handle_save_confirm_save(),
            SaveConfirmAction::Discard => root.handle_save_confirm_discard(),
            SaveConfirmAction::Cancel => root.handle_save_confirm_cancel(),
        }
        None
    }
}
