//! Sidebar 事件处理 — 分发器
//!
//! 按 Event 变体分组将处理逻辑委派到子模块。

mod color;
mod context_menu;
mod event_list;
mod group;
mod rename;
mod resize;
mod route;
mod track;

use crate::sidebar::Event;
use crate::sidebar::core::{Route, Sidebar};

impl Sidebar {
    pub fn update(&mut self, event: Event) -> bool {
        use Event::*;
        let prev_visible = self.panel_visible;
        let prev_route = self.route;
        let prev_group = self.active_group;
        let prev_context_menu_target = self.track_context_menu.target_track_id;
        let prev_renaming = self.renaming_track.as_ref().map(|(id, _)| *id);
        let prev_color_picking = self.color_picking_track;
        match event {
            // ── 分组切换（核心逻辑） ──
            GroupToggled(group) => self.handle_group_toggle(group),
            // ── 路由/面板 ──
            RouteUpdated(r) => self.handle_route_updated(r),
            PanelToggled(r) => self.handle_panel_toggled(r),
            // ── 音轨 ──
            TrackSelected(id) => self.handle_track_selected(id),
            TrackMuteToggled(id) => self.handle_track_mute_toggled(id),
            TrackSoloToggled(id) => self.handle_track_solo_toggled(id),
            TracksSelected(ids) => self.handle_tracks_selected(ids),
            AddTrack => self.handle_add_track(),
            TrackAddAbove(id) => self.handle_track_add_above(id),
            TrackAddBelow(id) => self.handle_track_add_below(id),
            TrackMoveUp(id) => self.handle_track_move_up(id),
            TrackMoveDown(id) => self.handle_track_move_down(id),
            // ── 音轨选项卡右键菜单 ──
            TrackContextMenuOpened(id) => self.handle_track_context_menu_opened(id),
            TrackContextMenuClosed => self.handle_track_context_menu_closed(),
            TrackContextMenuItemClicked(id, item) => {
                self.handle_track_context_menu_item_clicked(id, item)
            }
            // ── 音轨重命名 ──
            TrackRenameStarted(id) => self.handle_track_rename_started(id),
            TrackRenameChanged(id, value) => self.handle_track_rename_changed(id, value),
            TrackRenameConfirmed(id) => self.handle_track_rename_confirmed(id),
            TrackRenameCancelled(id) => self.handle_track_rename_cancelled(id),
            // ── 音轨颜色选择 ──
            TrackColorPickerOpened(id) => self.handle_track_color_picker_opened(id),
            TrackColorSelected(id, color) => self.handle_track_color_selected(id, color),
            TrackColorReset(id) => self.handle_track_color_reset(id),
            TrackColorPickerClosed(id) => self.handle_track_color_picker_closed(id),
            // ── 事件列表 ──
            EventListScrolled(offset, viewport_height) => {
                self.handle_event_list_scrolled(offset, viewport_height)
            }
            // ── 调整宽度 ──
            ResizeDragStarted(_) => self.handle_resize_drag_started(),
            ResizeDragged(_) => self.handle_resize_dragged(),
            ResizeDragEnded => self.handle_resize_drag_ended(),
            // ── 子按钮切换 ──
            AutomationPanelToggled => self.handle_automation_panel_toggled(),
            PianoRollToggled => self.handle_piano_roll_toggled(),
        }
        // 最终保护
        if self.route == Route::Arrangement {
            self.panel_visible = false;
        }

        self.panel_visible != prev_visible
            || self.route != prev_route
            || self.active_group != prev_group
            || self.track_context_menu.target_track_id != prev_context_menu_target
            || self.renaming_track.as_ref().map(|(id, _)| *id) != prev_renaming
            || self.color_picking_track != prev_color_picking
    }
}
