//! 音轨操作处理 — 选中、静音、独奏、添加、移动

use crate::event as ui_event;
use crate::sidebar::core::{Sidebar, Track};

impl Sidebar {
    /// 处理音轨选择
    pub(super) fn handle_track_selected(&mut self, id: usize) {
        tracing::debug!("Sidebar: 音轨选择 id={}", id);
        self.selected_track = id;
    }

    /// 处理静音切换
    pub(super) fn handle_track_mute_toggled(&mut self, id: usize) {
        if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
            track.is_muted = !track.is_muted;
        }
    }

    /// 处理独奏切换
    pub(super) fn handle_track_solo_toggled(&mut self, id: usize) {
        if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
            track.is_soloed = !track.is_soloed;
        }
    }

    /// 处理多轨选择
    pub(super) fn handle_tracks_selected(&mut self, ids: Vec<usize>) {
        if let Some(&first) = ids.first() {
            self.selected_track = first;
        }
    }

    /// 处理添加音轨
    pub(super) fn handle_add_track(&mut self) {
        let new_id = self.allocate_track_id();
        // yinhe 风格标签：端口字母 + 通道号（默认 port=0, channel=0）
        let display_label = "A01".to_string();
        self.tracks.push(Track {
            id: new_id,
            name: display_label.clone(),
            port: 0,
            channel: 0,
            display_label,
            is_conductor: false,
            can_delete: true,
            is_muted: false,
            is_soloed: false,
            color: None,
        });
        ui_event::emit(ui_event::Event::Window(
            ui_event::window::Event::local_track_added(new_id),
        ));
    }

    /// 处理在指定音轨上方添加
    pub(super) fn handle_track_add_above(&mut self, id: usize) {
        if let Some(idx) = self.tracks.iter().position(|t| t.id == id) {
            let new_id = self.allocate_track_id();
            let display_label = "A01".to_string();
            self.tracks.insert(
                idx,
                Track {
                    id: new_id,
                    name: display_label.clone(),
                    port: 0,
                    channel: 0,
                    display_label,
                    is_conductor: false,
                    can_delete: true,
                    is_muted: false,
                    is_soloed: false,
                    color: None,
                },
            );
            ui_event::emit(ui_event::Event::Window(
                ui_event::window::Event::local_track_added(new_id),
            ));
        }
    }

    /// 处理在指定音轨下方添加
    pub(super) fn handle_track_add_below(&mut self, id: usize) {
        if let Some(idx) = self.tracks.iter().position(|t| t.id == id) {
            let new_id = self.allocate_track_id();
            let display_label = "A01".to_string();
            let insert_idx = (idx + 1).min(self.tracks.len());
            self.tracks.insert(
                insert_idx,
                Track {
                    id: new_id,
                    name: display_label.clone(),
                    port: 0,
                    channel: 0,
                    display_label,
                    is_conductor: false,
                    can_delete: true,
                    is_muted: false,
                    is_soloed: false,
                    color: None,
                },
            );
            ui_event::emit(ui_event::Event::Window(
                ui_event::window::Event::local_track_added(new_id),
            ));
        }
    }

    /// 处理上移音轨
    pub(super) fn handle_track_move_up(&mut self, id: usize) {
        if let Some(idx) = self.tracks.iter().position(|t| t.id == id)
            && idx > 0
        {
            self.tracks.swap(idx, idx - 1);
        }
    }

    /// 处理下移音轨
    pub(super) fn handle_track_move_down(&mut self, id: usize) {
        if let Some(idx) = self.tracks.iter().position(|t| t.id == id)
            && idx + 1 < self.tracks.len()
        {
            self.tracks.swap(idx, idx + 1);
        }
    }
}
