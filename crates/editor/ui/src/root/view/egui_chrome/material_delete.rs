//! 素材删除确认：对话框状态 → egui 模型 + 动作回传（DRAW-045）。
//!
//! 数据源是对话框自己的 `state.material_delete_dialog`（由 Runner 在 UI
//! 就绪时注入下标与素材名快照）；主窗口的 `pending_delete` 只用于回执配对。

use lumino_message::MaterialDeleteDialogAction;
use lumino_ui_egui::chrome::MaterialDeleteModel;
use lumino_ui_egui::material_delete::MaterialDeleteAction;

use crate::root::Root;

impl Root {
    /// 素材删除确认对话框模型。
    pub(crate) fn material_delete_model(&self) -> MaterialDeleteModel {
        let state = &self.state.material_delete_dialog;
        let t = lumino_core::i18n::main_translations(self.settings.display.language);
        MaterialDeleteModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: t.material_delete_title.to_string(),
            name: state.name.clone(),
            warning: t.material_delete_warning.to_string(),
            cancel: t.material_delete_cancel.to_string(),
            delete: t.material_delete.to_string(),
            index: state.index,
        }
    }

    /// 对话框动作 → 生产消息。
    pub(crate) fn material_delete_action_message(
        &self,
        action: MaterialDeleteAction,
    ) -> crate::Message {
        crate::Message::MaterialDelete(match action {
            MaterialDeleteAction::Cancel => MaterialDeleteDialogAction::Cancel,
            MaterialDeleteAction::Confirm(index) => MaterialDeleteDialogAction::Confirm(index),
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
            DialogType::MaterialDelete,
        )
    }

    #[test]
    fn test_model_mirrors_dialog_state() {
        let mut root = root();
        root.set_material_delete_dialog(2, "钢琴音色.json".to_string());

        let model = root.material_delete_model();
        assert_eq!(model.index, 2);
        assert_eq!(model.name, "钢琴音色.json");
        assert!(!model.title.is_empty());
        assert!(!model.warning.is_empty());
        assert!(!model.cancel.is_empty() && !model.delete.is_empty());
    }

    #[test]
    fn test_model_index_defaults_without_injection() {
        // 对话框 UI 就绪前可能先渲染一帧：模型必须是可绘制的空数据，而不是 panic。
        let model = root().material_delete_model();
        assert_eq!(model.index, 0);
        assert!(model.name.is_empty());
    }

    #[test]
    fn test_actions_map_to_dialog_action_messages() {
        let root = root();
        assert!(matches!(
            root.material_delete_action_message(MaterialDeleteAction::Cancel),
            crate::Message::MaterialDelete(MaterialDeleteDialogAction::Cancel)
        ));
        assert!(matches!(
            root.material_delete_action_message(MaterialDeleteAction::Confirm(7)),
            crate::Message::MaterialDelete(MaterialDeleteDialogAction::Confirm(7))
        ));
    }
}
