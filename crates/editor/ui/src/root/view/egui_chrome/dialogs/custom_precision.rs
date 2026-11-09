//! 自定义精度对话框：生产状态 → egui 模型，以及动作 → `Message` 映射。

use lumino_core::tr;
use lumino_extras::i18n::main_translations;
use lumino_ui_egui::chrome::{CustomPrecisionModel, DotOptionModel};

use crate::Message;

impl crate::root::Root {
    /// 自定义精度模型：三连音数 / 分音符 / 附点 / 除数 / 错误 / 按钮。
    pub(super) fn custom_precision_model(&self) -> CustomPrecisionModel {
        let language = self.settings.display.language;
        let t = main_translations(language);
        let state = &self.state.custom_precision_dialog;
        CustomPrecisionModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            tuplet_count: state.tuplet_count.clone(),
            note_value: state.note_value.clone(),
            note_label: t.precision_note_label.to_owned(),
            dot_label: tr!("附点").to_string(),
            dots: lumino_core::DotType::all()
                .iter()
                .copied()
                .map(|dot| DotOptionModel {
                    dot,
                    label: lumino_extras::i18n::dot_type_name(dot, language).to_owned(),
                    selected: state.dot_type == dot,
                })
                .collect(),
            divide_label: t.precision_divide_by.to_owned(),
            divisor: state.divisor.clone(),
            error: state.error.clone(),
            ok: t.precision_ok.to_owned(),
            cancel: t.precision_cancel.to_owned(),
        }
    }

    /// egui 自定义精度动作 → 生产 `Message`。
    pub(crate) fn custom_precision_action_message(
        &self,
        action: &lumino_ui_egui::custom_precision::CustomPrecisionAction,
    ) -> Option<Message> {
        use crate::message::CustomPrecisionAction as Prod;
        use lumino_ui_egui::custom_precision::CustomPrecisionAction as Action;
        Some(Message::CustomPrecision(match action {
            Action::TupletCountChanged(text) => Prod::TupletCountChanged(text.clone()),
            Action::NoteValueChanged(text) => Prod::NoteValueChanged(text.clone()),
            Action::DotTypeChanged(dot) => Prod::DotTypeChanged(*dot),
            Action::DivisorChanged(text) => Prod::DivisorChanged(text.clone()),
            Action::Confirm => Prod::Confirm,
            Action::Cancel => Prod::CloseDialog,
        }))
    }
}
