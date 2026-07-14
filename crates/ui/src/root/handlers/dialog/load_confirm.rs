//! 加载确认对话框处理器

use crate::message::{LoadConfirmAction, Message};
use crate::root::Root;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_load_confirm(
        &self,
        root: &mut Root,
        action: LoadConfirmAction,
    ) -> Option<Message> {
        match action {
            LoadConfirmAction::Confirm => {
                root.handle_confirm_load();
            }
            LoadConfirmAction::CloseDialog => {
                root.handle_cancel_load();
            }
        }
        None
    }
}
