//! 画刷设置对话框：生产草稿 → egui 模型，以及动作 → `Message` 映射。

use lumino_core::{BrushConfig, tr, trf};
use lumino_ui_egui::chrome::{BrushLevelModel, BrushSettingsModel, BrushTrackChoice};

use crate::Message;

impl crate::root::Root {
    /// 画刷设置模型：逐层音轨分配 + 空态 + 按钮。
    pub(super) fn brush_settings_model(&self) -> BrushSettingsModel {
        let draft = &self.state.brush_settings_draft;
        let tracks = &self.state.brush_settings_tracks;
        let default_label = tr!("默认（自动分配）").to_string();
        let can_remove = draft.tracks.len() > BrushConfig::MIN_THICKNESS as usize;

        let levels = (0..draft.tracks.len())
            .map(|level| {
                let assigned = draft.tracks.get(level).copied().flatten();
                let mut choices = vec![BrushTrackChoice {
                    id: None,
                    label: default_label.clone(),
                    selected: assigned.is_none(),
                }];
                choices.extend(tracks.iter().map(|(id, name)| BrushTrackChoice {
                    id: Some(*id),
                    label: name.clone(),
                    selected: assigned == Some(*id),
                }));
                BrushLevelModel {
                    index: level,
                    label: trf!("第 {n} 层", n = level + 1),
                    tracks: choices,
                    can_remove,
                }
            })
            .collect();

        BrushSettingsModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("画刷绘制行为设置").to_string(),
            levels,
            has_tracks: !tracks.is_empty(),
            empty: tr!("无可用音轨，请先在工程中创建普通音轨。").to_string(),
            save: tr!("保存").to_string(),
            cancel: tr!("取消").to_string(),
        }
    }

    /// egui 画刷设置动作 → 生产 `Message`。
    pub(crate) fn brush_settings_action_message(
        &self,
        action: &lumino_ui_egui::brush_settings::BrushSettingsAction,
    ) -> Option<Message> {
        use crate::message::BrushSettingsAction as Prod;
        use lumino_ui_egui::brush_settings::BrushSettingsAction as Action;
        Some(Message::BrushSettings(match action {
            Action::LevelTrackChanged(level, id) => Prod::LevelTrackChanged(*level, *id),
            Action::AddLevel(level) => Prod::AddLevel(*level),
            Action::RemoveLevel(level) => Prod::RemoveLevel(*level),
            Action::Save => Prod::Save,
            Action::Cancel => Prod::Cancel,
        }))
    }
}
