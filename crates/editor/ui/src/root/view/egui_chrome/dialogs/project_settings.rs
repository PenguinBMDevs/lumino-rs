//! 工程设置对话框：生产状态 → egui 模型，以及动作 → `Message` 映射。
//!
//! 单独成文件把 `dialogs.rs` 保持在 400 行以内。

use lumino_core::tr;
use lumino_extras::i18n::main_translations;
use lumino_ui_egui::chrome::{ProjectNoteCountModel, ProjectSettingsModel};

use crate::Message;
use crate::root::Root;

impl Root {
    /// 工程设置对话框模型（系统原生标题栏 / 自制标题栏，整窗内容）。
    pub(super) fn project_settings_model(&self) -> ProjectSettingsModel {
        let t = main_translations(self.settings.display.language);
        let dialog = &self.state.project_settings_dialog;
        ProjectSettingsModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: t.project_title.to_owned(),
            name_label: t.project_name_label.to_owned(),
            name: dialog.title.clone(),
            name_hint: t.project_name_placeholder.to_owned(),
            bpm_label: t.project_bpm_label.to_owned(),
            bpm: dialog.tempo.clone(),
            copyright_label: t.project_copyright_label.to_owned(),
            copyright: dialog.copyright.clone(),
            copyright_hint: t.project_copyright_placeholder.to_owned(),
            author_label: t.project_author_label.to_owned(),
            author: dialog.author.clone(),
            author_hint: t.project_author_placeholder.to_owned(),
            time_signature_label: tr!("拍号").to_string(),
            numerator: dialog.time_signature_numerator.clone(),
            denominator: dialog.time_signature_denominator.clone(),
            use10: dialog.use10,
            use10_label: tr!("启用通道 10").to_string(),
            use10_hint: tr!("区域色号上限 16").to_string(),
            created_label: t.project_created_label.to_owned(),
            created: if dialog.created_display.is_empty() {
                t.project_unknown.to_owned()
            } else {
                dialog.created_display.clone()
            },
            editing_time_label: t.project_editing_time_label.to_owned(),
            editing_time: dialog.format_editing_time(),
            note_count: (dialog.note_count_display
                == lumino_core::storage::config::NoteCountDisplay::ProjectSettings)
                .then(|| ProjectNoteCountModel {
                    label: t.note_count_label.to_owned(),
                    value: lumino_extras::i18n::format_thousands(dialog.note_count as u64),
                }),
            ok: t.project_ok.to_owned(),
            cancel: t.project_cancel.to_owned(),
        }
    }

    /// egui 工程设置动作 → 生产 `Message`。
    pub(crate) fn project_settings_action_message(
        &self,
        action: &lumino_ui_egui::project_settings::ProjectSettingsAction,
    ) -> Option<Message> {
        use crate::message::ProjectSettingsAction as Prod;
        use lumino_ui_egui::project_settings::ProjectSettingsAction as Action;
        Some(match action {
            Action::NameChanged(value) => {
                Message::ProjectSettings(Prod::TitleChanged(value.clone()))
            }
            Action::TempoChanged(value) => {
                Message::ProjectSettings(Prod::TempoChanged(value.clone()))
            }
            Action::CopyrightChanged(value) => {
                Message::ProjectSettings(Prod::CopyrightChanged(value.clone()))
            }
            Action::AuthorChanged(value) => {
                Message::ProjectSettings(Prod::AuthorChanged(value.clone()))
            }
            Action::NumeratorChanged(value) => {
                Message::ProjectSettings(Prod::TimeSignatureNumeratorChanged(value.clone()))
            }
            Action::DenominatorChanged(value) => {
                Message::ProjectSettings(Prod::TimeSignatureDenominatorChanged(value.clone()))
            }
            Action::Use10Changed(value) => Message::ProjectSettings(Prod::Use10Changed(*value)),
            Action::Confirm => Message::ProjectSettings(Prod::Confirm),
            Action::Cancel => Message::ProjectSettings(Prod::CloseDialog),
        })
    }
}
