//! 找回删除音轨：生产状态 → egui 模型，以及动作 → `Message` 映射。

use lumino_core::tr;
use lumino_ui_egui::chrome::{RecoverTrackModel, RecoverTrackRowModel, RecoverTrackSelection};

use crate::Message;

impl crate::root::Root {
    /// 找回删除音轨模型：标题 / 提示 / 表头 / 条目 / 选中条目 / 按钮。
    pub(super) fn recover_track_model(&self) -> RecoverTrackModel {
        let state = &self.state.recover_track_dialog;
        let selected_entry = state.selected_index.and_then(|i| state.entries.get(i));
        RecoverTrackModel {
            titlebar: !self.settings.synth.use_native_titlebar,
            title: tr!("找回删除音轨").to_string(),
            hint: tr!(
                "选中条目后点击「恢复」可还原至原位置；「永久删除」将销毁缓存文件并释放轨道编号。"
            )
            .to_string(),
            filename_header: tr!("文件名").to_string(),
            deleted_header: tr!("删除时间").to_string(),
            notes_header: tr!("音符总数").to_string(),
            entries: state
                .entries
                .iter()
                .enumerate()
                .map(|(index, entry)| RecoverTrackRowModel {
                    filename: entry.filename.clone(),
                    deleted_at: entry.deleted_at.clone(),
                    note_count: entry.note_count.to_string(),
                    selected: state.selected_index == Some(index),
                })
                .collect(),
            empty: tr!("暂无已删除的音轨缓存").to_string(),
            selected: selected_entry.map(|entry| RecoverTrackSelection {
                path: entry.path.display().to_string(),
                original_index: entry.original_index,
                track_id: entry.track_id,
            }),
            can_act: selected_entry.is_some(),
            restore: tr!("恢复").to_string(),
            delete: tr!("永久删除").to_string(),
            close: tr!("关闭").to_string(),
        }
    }

    /// egui 找回音轨动作 → 生产 `Message`。
    pub(crate) fn recover_track_action_message(
        &self,
        action: &lumino_ui_egui::recover_track::RecoverTrackAction,
    ) -> Option<Message> {
        use crate::message::RecoverTrackAction as Prod;
        use lumino_ui_egui::recover_track::RecoverTrackAction as Action;
        Some(Message::RecoverTrack(match action {
            Action::SelectionChanged(index) => Prod::SelectionChanged(*index),
            Action::Restore {
                path,
                original_index,
            } => Prod::Restore {
                path: std::path::PathBuf::from(path),
                original_index: *original_index,
            },
            Action::PermanentlyDelete { path, track_id } => Prod::PermanentlyDelete {
                path: std::path::PathBuf::from(path),
                track_id: *track_id,
            },
            Action::CloseDialog => Prod::CloseDialog,
        }))
    }
}
