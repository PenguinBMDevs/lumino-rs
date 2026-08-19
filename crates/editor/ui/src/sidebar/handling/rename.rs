//! 音轨重命名处理 — 开始、输入、确认、取消

use crate::sidebar::core::{Sidebar, TrackContextMenuState};

impl Sidebar {
    /// 处理开始重命名音轨
    pub(super) fn handle_track_rename_started(&mut self, id: usize) {
        if let Some(track) = self.tracks.iter().find(|t| t.id == id) {
            self.renaming_track = Some((id, track.name.clone()));
        }
        self.track_context_menu = TrackContextMenuState::default();
    }

    /// 处理重命名输入变化
    pub(super) fn handle_track_rename_changed(&mut self, id: usize, value: String) {
        if let Some((renaming_id, buffer)) = &mut self.renaming_track
            && *renaming_id == id
        {
            *buffer = value;
        }
    }

    /// 处理确认重命名
    pub(super) fn handle_track_rename_confirmed(&mut self, id: usize) {
        if let Some((renaming_id, buffer)) = self.renaming_track.take()
            && renaming_id == id
            && let Some(track) = self.tracks.iter_mut().find(|t| t.id == id)
        {
            track.name = buffer;
        }
    }

    /// 处理取消重命名
    pub(super) fn handle_track_rename_cancelled(&mut self, _id: usize) {
        self.renaming_track = None;
    }
}
