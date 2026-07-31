//! 音轨上下文菜单处理 — 打开、关闭、菜单项点击

use crate::sidebar::core::{Sidebar, TrackContextMenuState};
use lumino_message::TrackContextMenuItem;

impl Sidebar {
    /// 处理打开音轨选项卡右键菜单
    pub(super) fn handle_track_context_menu_opened(&mut self, id: usize) {
        self.track_context_menu = TrackContextMenuState {
            target_track_id: Some(id),
        };
        self.renaming_track = None;
        self.color_picking_track = None;
    }

    /// 处理关闭音轨选项卡右键菜单
    pub(super) fn handle_track_context_menu_closed(&mut self) {
        self.track_context_menu = TrackContextMenuState::default();
    }

    /// 处理点击音轨选项卡右键菜单项
    pub(super) fn handle_track_context_menu_item_clicked(
        &mut self,
        id: usize,
        item: TrackContextMenuItem,
    ) {
        self.track_context_menu = TrackContextMenuState::default();
        match item {
            TrackContextMenuItem::Delete => {
                if let Some(idx) = self.tracks.iter().position(|t| t.id == id)
                    && self.tracks[idx].can_delete
                {
                    self.tracks.remove(idx);
                    if self.selected_track == id
                        || !self.tracks.iter().any(|t| t.id == self.selected_track)
                    {
                        self.selected_track = self.tracks.first().map(|t| t.id).unwrap_or(0);
                    }
                    self.renaming_track = None;
                    self.color_picking_track = None;
                }
            }
            TrackContextMenuItem::Rename => {
                if let Some(track) = self.tracks.iter().find(|t| t.id == id) {
                    self.renaming_track = Some((id, track.name.clone()));
                }
                self.color_picking_track = None;
            }
            TrackContextMenuItem::SetColor => {
                self.color_picking_track = Some(id);
                self.renaming_track = None;
            }
            TrackContextMenuItem::SetChannel => {
                tracing::info!("设置通道功能待实现，音轨 id={}", id);
            }
        }
    }
}
