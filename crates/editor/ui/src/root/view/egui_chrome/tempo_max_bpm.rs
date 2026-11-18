//! 自定义 BPM 上限对话框：对话框状态 → egui 模型 + 动作回传（DRAW-045）。

use lumino_message::TempoMaxBpmDialogAction;
use lumino_ui_egui::chrome::TempoMaxBpmModel;
use lumino_ui_egui::tempo_max_bpm::TempoMaxBpmAction;

use crate::root::Root;

impl Root {
    /// 自定义 BPM 上限对话框模型。
    pub(crate) fn tempo_max_bpm_model(&self) -> TempoMaxBpmModel {
        let state = &self.state.tempo_max_bpm_dialog;
        let t = lumino_extras::i18n::settings_translations(self.settings.display.language);
        TempoMaxBpmModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: lumino_core::tr!("自定义 BPM 上限").to_string(),
            label: t.editing_tempo_max_bpm.to_owned(),
            input: state.input.clone(),
            hint: lumino_core::tr!("上限越大，Tempo 曲线越平坦").to_string(),
            error: state.error.clone(),
            ok: t.confirm.to_owned(),
            cancel: t.cancel.to_owned(),
        }
    }

    /// 对话框动作 → 生产消息。
    pub(crate) fn tempo_max_bpm_action_message(&self, action: TempoMaxBpmAction) -> crate::Message {
        crate::Message::TempoMaxBpm(match action {
            TempoMaxBpmAction::Input(value) => TempoMaxBpmDialogAction::Input(value),
            TempoMaxBpmAction::Confirm => TempoMaxBpmDialogAction::Confirm,
            TempoMaxBpmAction::Cancel => TempoMaxBpmDialogAction::Cancel,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::root_state::DialogType;

    fn root() -> Root {
        Root::new_dialog(
            &lumino_ui_core::Theme::TokyoNightStorm.to_string(),
            DialogType::TempoMaxBpm,
        )
    }

    #[test]
    fn test_model_mirrors_dialog_state() {
        let mut root = root();
        root.set_tempo_max_bpm_dialog(700.0);

        let model = root.tempo_max_bpm_model();
        assert_eq!(model.input, "700");
        assert!(model.error.is_none());
        assert!(!model.title.is_empty() && !model.label.is_empty());
        assert!(!model.ok.is_empty() && !model.cancel.is_empty());
    }

    #[test]
    fn test_model_carries_error_after_invalid_confirm() {
        let mut root = root();
        root.set_tempo_max_bpm_dialog(512.0);
        root.state.tempo_max_bpm_dialog.input = "abc".to_string();
        root.update(crate::Message::TempoMaxBpm(
            TempoMaxBpmDialogAction::Confirm,
        ));
        let model = root.tempo_max_bpm_model();
        assert!(
            model.error.is_some(),
            "非法输入确认后必须带错误提示，窗口保持打开"
        );
    }

    #[test]
    fn test_actions_map_to_dialog_action_messages() {
        let root = root();
        assert!(matches!(
            root.tempo_max_bpm_action_message(TempoMaxBpmAction::Input("1024".to_string())),
            crate::Message::TempoMaxBpm(TempoMaxBpmDialogAction::Input(value)) if value == "1024"
        ));
        assert!(matches!(
            root.tempo_max_bpm_action_message(TempoMaxBpmAction::Confirm),
            crate::Message::TempoMaxBpm(TempoMaxBpmDialogAction::Confirm)
        ));
        assert!(matches!(
            root.tempo_max_bpm_action_message(TempoMaxBpmAction::Cancel),
            crate::Message::TempoMaxBpm(TempoMaxBpmDialogAction::Cancel)
        ));
    }
}
