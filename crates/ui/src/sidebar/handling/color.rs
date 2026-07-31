//! 音轨颜色选择处理 — 打开选择器、选择颜色、重置、关闭

use crate::sidebar::core::{Sidebar, TrackContextMenuState};
use iced_core::Color;

impl Sidebar {
    /// 处理打开颜色选择器
    pub(super) fn handle_track_color_picker_opened(&mut self, id: usize) {
        self.color_picking_track = Some(id);
        self.track_context_menu = TrackContextMenuState::default();
    }

    /// 处理选择音轨颜色
    pub(super) fn handle_track_color_selected(&mut self, id: usize, color: Color) {
        if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
            track.color = Some(color);
        }
        self.color_picking_track = None;
    }

    /// 处理重置音轨颜色为默认
    pub(super) fn handle_track_color_reset(&mut self, id: usize) {
        if let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) {
            track.color = None;
        }
        self.color_picking_track = None;
    }

    /// 处理关闭颜色选择器
    pub(super) fn handle_track_color_picker_closed(&mut self, _id: usize) {
        self.color_picking_track = None;
    }
}
