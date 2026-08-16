//! 设置对话框处理器

use crate::host::DialogResult;
use crate::message::{Message, SettingsDialogAction};
use crate::root::Root;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_settings_dialog(
        &self,
        root: &mut Root,
        action: SettingsDialogAction,
    ) -> Option<Message> {
        match action {
            SettingsDialogAction::OpenDialog => {
                root.state.dialog_type = crate::state::root_state::DialogType::Settings;
            }
            SettingsDialogAction::CloseDialog => {
                // 返回设置结果，将设置同步到主窗口
                root.state.dialog_result = Some(DialogResult::Settings {
                    settings: root.settings.clone(),
                    theme: root.window.theme.to_string(),
                });
            }
        }
        None
    }
}
