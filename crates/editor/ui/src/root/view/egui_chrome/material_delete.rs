//! 素材删除确认：生产状态 → egui 模型 + 动作回传。
//!
//! 由 `right_sidebar.materials.pending_delete` 驱动（素材名为快照，列表刷新后仍可正确展示）。

use lumino_message::RightSidebarAction;
use lumino_ui_egui::material_delete::{MaterialDeleteAction, MaterialDeleteModel};

use crate::root::Root;

impl Root {
    /// 删除确认模型；无待确认项时返回 `None`。
    pub(crate) fn material_delete_model(&self) -> Option<MaterialDeleteModel> {
        let index = self.right_sidebar.materials.pending_delete?;
        let name = self
            .right_sidebar
            .materials
            .pending_delete_name
            .clone()
            .or_else(|| {
                self.right_sidebar
                    .materials
                    .entries
                    .get(index)
                    .map(|e| e.name.clone())
            })
            .unwrap_or_default();
        let t = lumino_extras::i18n::main_translations(self.settings.display.language);
        Some(MaterialDeleteModel {
            title: t.material_delete_title.to_string(),
            name,
            warning: t.material_delete_warning.to_string(),
            cancel: t.material_delete_cancel.to_string(),
            delete: t.material_delete.to_string(),
            index,
        })
    }

    /// 动作 → 生产消息。
    pub(crate) fn material_delete_action_message(
        &self,
        action: MaterialDeleteAction,
    ) -> crate::Message {
        crate::Message::RightSidebar(match action {
            MaterialDeleteAction::Cancel => RightSidebarAction::MaterialDeleteCancelled,
            MaterialDeleteAction::Confirm(index) => {
                RightSidebarAction::MaterialDeleteConfirmed(index)
            }
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
            DialogType::VideoExport,
        )
    }

    #[test]
    fn model_mirrors_pending_delete() {
        let mut root = root();
        root.right_sidebar.materials.pending_delete = Some(2);
        root.right_sidebar.materials.pending_delete_name = Some("钢琴音色.json".into());

        let model = root
            .material_delete_model()
            .expect("有待确认项时应产出模型");
        assert_eq!(model.index, 2);
        assert_eq!(model.name, "钢琴音色.json");
        assert!(!model.title.is_empty());
        assert!(!model.warning.is_empty());
        assert!(!model.cancel.is_empty() && !model.delete.is_empty());
    }

    #[test]
    fn no_pending_delete_means_no_model() {
        let mut root = root();
        root.right_sidebar.materials.pending_delete = None;
        assert!(root.material_delete_model().is_none());
    }

    #[test]
    fn actions_map_to_right_sidebar_messages() {
        let root = root();
        assert!(matches!(
            root.material_delete_action_message(MaterialDeleteAction::Cancel),
            crate::Message::RightSidebar(RightSidebarAction::MaterialDeleteCancelled)
        ));
        assert!(matches!(
            root.material_delete_action_message(MaterialDeleteAction::Confirm(7)),
            crate::Message::RightSidebar(RightSidebarAction::MaterialDeleteConfirmed(7))
        ));
    }
}
