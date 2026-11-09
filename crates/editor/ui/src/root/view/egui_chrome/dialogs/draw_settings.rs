//! 音符画设置对话框：生产文案 → egui 模型，以及关闭动作 → `Message`。

use lumino_extras::i18n::main_translations;
use lumino_ui_egui::chrome::DrawSettingsModel;

use crate::Message;

impl crate::root::Root {
    /// 音符画设置模型：标题 / 占位说明 / 关闭（三处文案与生产同源）。
    pub(super) fn draw_settings_model(&self) -> DrawSettingsModel {
        let t = main_translations(self.settings.display.language);
        DrawSettingsModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: t.tool_panel_settings.to_owned(),
            placeholder: t.draw_settings_placeholder.to_owned(),
            close: t.file_close.to_owned(),
        }
    }

    /// egui 音符画设置动作 → 生产 `Message`。
    pub(crate) fn draw_settings_action_message(
        &self,
        action: &lumino_ui_egui::draw_settings::DrawSettingsAction,
    ) -> Option<Message> {
        use lumino_ui_egui::draw_settings::DrawSettingsAction as Action;
        Some(match action {
            Action::Close => Message::DrawSettings(crate::message::DrawSettingsAction::CloseDialog),
        })
    }
}
