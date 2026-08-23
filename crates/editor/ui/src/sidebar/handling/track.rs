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
    ///
    /// Conductor 首位不变量：不允许在 conductor 上方插入，
    /// 目标为 conductor（或目标位于 conductor 之前）时改为插入到 conductor 之后。
    pub(super) fn handle_track_add_above(&mut self, id: usize) {
        if let Some(idx) = self.tracks.iter().position(|t| t.id == id) {
            let conductor_idx = self.tracks.iter().position(|t| t.is_conductor);
            let insert_idx = match conductor_idx {
                Some(ci) => idx.max(ci + 1),
                None => idx,
            };
            let new_id = self.allocate_track_id();
            let display_label = "A01".to_string();
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

    /// 处理上移音轨（Conductor 首位不变量：conductor 不可移动，且目标位置不能是 conductor）
    pub(super) fn handle_track_move_up(&mut self, id: usize) {
        if let Some(idx) = self.tracks.iter().position(|t| t.id == id)
            && idx > 0
            && !self.tracks[idx].is_conductor
            && !self.tracks[idx - 1].is_conductor
        {
            self.tracks.swap(idx, idx - 1);
        }
    }

    /// 处理下移音轨（Conductor 不可移动，保持首位）
    pub(super) fn handle_track_move_down(&mut self, id: usize) {
        if let Some(idx) = self.tracks.iter().position(|t| t.id == id)
            && idx + 1 < self.tracks.len()
            && !self.tracks[idx].is_conductor
        {
            self.tracks.swap(idx, idx + 1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidebar::core::Track;

    /// 构造仅含指定 id 音轨的 Sidebar（id=0 为 conductor）
    fn sidebar_with_ids(ids: &[usize]) -> Sidebar {
        let mut s = Sidebar::new();
        s.tracks = ids
            .iter()
            .map(|id| Track {
                id: *id,
                name: format!("Track{}", id),
                port: 0,
                channel: 0,
                display_label: "A01".to_string(),
                is_conductor: *id == 0,
                can_delete: *id != 0,
                is_muted: false,
                is_soloed: false,
                color: None,
            })
            .collect();
        s.next_track_id = ids.iter().copied().max().unwrap_or(0) + 1;
        s
    }

    fn ids(s: &Sidebar) -> Vec<usize> {
        s.tracks.iter().map(|t| t.id).collect()
    }

    #[test]
    fn test_move_up_down_respects_conductor_guard() {
        let mut s = sidebar_with_ids(&[0, 1, 2, 3]);
        // conductor 不能上移（本来就在首）也不能下移
        s.handle_track_move_down(0);
        assert_eq!(ids(&s), vec![0, 1, 2, 3], "conductor 不能被下移挤占");
        // 其他音轨正常移动
        s.handle_track_move_down(1);
        assert_eq!(ids(&s), vec![0, 2, 1, 3]);
        s.handle_track_move_up(1);
        assert_eq!(ids(&s), vec![0, 1, 2, 3]);
    }

    #[test]
    fn test_move_up_never_puts_track_above_conductor() {
        // 防御场景：conductor 不在首位时，上移也不允许越过 conductor
        let mut s = sidebar_with_ids(&[1, 0, 2]); // conductor(id=0) 在索引 1
        s.handle_track_move_up(2); // 索引 2 上移 → 与索引 1（conductor）交换？被禁止？
        assert_eq!(ids(&s), vec![1, 0, 2], "conductor 上方不允许出现其他音轨");
    }

    #[test]
    fn test_add_above_conductor_inserts_after_it() {
        let mut s = sidebar_with_ids(&[0, 1, 2]);
        s.handle_track_add_above(0); // 在 conductor 上方添加 → 实际插到其后
        assert_eq!(ids(&s), vec![0, 3, 1, 2], "新音轨应插入 conductor 之后");
    }

    #[test]
    fn test_handle_track_solo_toggle() {
        let mut s = sidebar_with_ids(&[0, 1, 2]);
        assert!(!s.tracks[1].is_soloed, "初始不应独奏");
        s.handle_track_solo_toggled(1);
        assert!(s.tracks[1].is_soloed, "首次切换应进入独奏");
        s.handle_track_solo_toggled(1);
        assert!(!s.tracks[1].is_soloed, "再次切换应取消独奏");
    }

    #[test]
    fn test_handle_track_mute_toggle() {
        let mut s = sidebar_with_ids(&[0, 1, 2]);
        assert!(!s.tracks[1].is_muted, "初始不应静音");
        s.handle_track_mute_toggled(1);
        assert!(s.tracks[1].is_muted, "切换应进入静音");
        s.handle_track_mute_toggled(1);
        assert!(!s.tracks[1].is_muted, "再次切换应取消静音");
    }
}
