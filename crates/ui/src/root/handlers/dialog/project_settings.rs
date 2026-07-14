//! 工程设置对话框处理器

use crate::host::DialogResult;
use crate::message::{Message, ProjectSettingsAction};
use crate::root::Root;

use super::DialogHandler;

impl DialogHandler {
    pub(super) fn handle_project_settings(
        &self,
        root: &mut Root,
        action: ProjectSettingsAction,
    ) -> Option<Message> {
        match action {
            ProjectSettingsAction::OpenDialog => {
                root.state.project_settings_dialog.is_open = true;
            }
            ProjectSettingsAction::CloseDialog => {
                root.state.project_settings_dialog.is_open = false;
                root.state.dialog_result = Some(DialogResult::Cancel);
            }
            ProjectSettingsAction::Confirm => {
                self.handle_confirm_project_settings(root);
            }
            ProjectSettingsAction::TitleChanged(value) => {
                root.state.project_settings_dialog.title = value;
            }
            ProjectSettingsAction::TempoChanged(value) => {
                // 只允许数字和小数点
                if value.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    root.state.project_settings_dialog.tempo = value;
                }
            }
            ProjectSettingsAction::CopyrightChanged(value) => {
                root.state.project_settings_dialog.copyright = value;
            }
        }
        None
    }

    fn handle_confirm_project_settings(&self, root: &mut Root) {
        let dialog = &root.state.project_settings_dialog;

        // 验证 BPM 值
        if let Some(tempo) = dialog.parse_tempo() {
            let title = dialog.title.clone();
            let copyright = dialog.copyright.clone();

            // 设置对话框结果（触发窗口关闭 + 主窗口处理）
            root.state.dialog_result = Some(DialogResult::ProjectSettings {
                title,
                tempo,
                copyright,
            });
            root.state.project_settings_dialog.is_open = false;
        } else {
            tracing::warn!("工程设置: BPM 值无效: {}", dialog.tempo);
        }
    }
}
