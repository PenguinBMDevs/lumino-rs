//! 对话框管理处理器

use crate::host::DialogResult;
use crate::message::Message;
use crate::root::Root;
use crate::root::handlers::MessageHandler;

/// 对话框消息处理器
pub struct DialogHandler;

impl DialogHandler {
    pub fn new() -> Self {
        Self
    }

    fn handle_custom_precision_dialog_open(&self, root: &mut Root) {
        root.state.custom_precision_dialog.is_open = true;
    }

    fn handle_custom_precision_dialog_close(&self, root: &mut Root) {
        root.state.custom_precision_dialog.is_open = false;
    }

    fn handle_confirm_custom_precision(&self, root: &mut Root) {
        let dialog = &root.state.custom_precision_dialog;

        if let Some(ticks) = dialog.calculate_ticks(root.editor.editor_state.view.ppq as u32) {
            root.editor.set_snap_precision(ticks);
            root.editor.set_default_note_length(ticks);
            tracing::info!(
                "自定义精度已应用: {} ticks (PPQ={})",
                ticks,
                root.editor.editor_state.view.ppq
            );
        }

        root.state.custom_precision_dialog.is_open = false;
    }

    fn update_precision_if_digit(target: &mut String, value: &str) {
        if value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
            *target = value.to_string();
        }
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

impl Default for DialogHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl MessageHandler for DialogHandler {
    fn handle(&mut self, root: &mut Root, msg: Message) -> Option<Message> {
        match msg {
            Message::OpenCustomPrecisionDialog => {
                self.handle_custom_precision_dialog_open(root);
                None
            }
            Message::CloseCustomPrecisionDialog => {
                self.handle_custom_precision_dialog_close(root);
                None
            }
            Message::ConfirmCustomPrecision => {
                self.handle_confirm_custom_precision(root);
                None
            }
            Message::CustomPrecisionTupletCountChanged(value) => {
                Self::update_precision_if_digit(
                    &mut root.state.custom_precision_dialog.tuplet_count,
                    &value,
                );
                None
            }
            Message::CustomPrecisionTupletTypeChanged(value) => {
                root.state.custom_precision_dialog.tuplet_type = value;
                root.state.custom_precision_dialog.tuplet_count = value.value().to_string();
                None
            }
            Message::CustomPrecisionDotTypeChanged(value) => {
                root.state.custom_precision_dialog.dot_type = value;
                None
            }
            Message::CustomPrecisionNoteValueChanged(value) => {
                Self::update_precision_if_digit(
                    &mut root.state.custom_precision_dialog.note_value,
                    &value,
                );
                None
            }
            Message::CustomPrecisionDivisorChanged(value) => {
                Self::update_precision_if_digit(
                    &mut root.state.custom_precision_dialog.divisor,
                    &value,
                );
                None
            }
            Message::ConfirmLoadConfirm => {
                root.handle_confirm_load();
                None
            }
            Message::CloseLoadConfirmDialog => {
                root.handle_cancel_load();
                None
            }
            // 工程设置对话框消息
            Message::OpenProjectSettingsDialog => {
                root.state.project_settings_dialog.is_open = true;
                None
            }
            Message::CloseProjectSettingsDialog => {
                root.state.project_settings_dialog.is_open = false;
                root.state.dialog_result = Some(DialogResult::Cancel);
                None
            }
            Message::ConfirmProjectSettings => {
                self.handle_confirm_project_settings(root);
                None
            }
            Message::ProjectSettingsTitleChanged(value) => {
                root.state.project_settings_dialog.title = value;
                None
            }
            Message::ProjectSettingsTempoChanged(value) => {
                // 只允许数字和小数点
                if value.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    root.state.project_settings_dialog.tempo = value;
                }
                None
            }
            Message::ProjectSettingsCopyrightChanged(value) => {
                root.state.project_settings_dialog.copyright = value;
                None
            }
            // 设置对话框消息
            Message::OpenSettingsDialog => {
                root.state.dialog_type = crate::state::root_state::DialogType::Settings;
                None
            }
            Message::CloseSettingsDialog => {
                // 返回设置结果，将设置同步到主窗口
                root.state.dialog_result = Some(DialogResult::Settings {
                    settings: root.settings.clone(),
                    theme: root.window.theme.to_string(),
                });
                root.state.dialog_type = crate::state::root_state::DialogType::None;
                None
            }

            other => Some(other),
        }
    }
}
