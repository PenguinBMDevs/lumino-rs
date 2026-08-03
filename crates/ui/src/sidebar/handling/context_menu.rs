//! 音轨上下文菜单处理 — 打开、关闭、菜单项点击

use crate::sidebar::core::{PendingTrackDeletionMeta, Sidebar, TrackContextMenuState};
use lumino_message::{PanelContextMenuItem, TrackContextMenuItem};

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
                    // 在移除前缓存元数据（移除后无法再从 tracks 中查询）
                    let track = self.tracks[idx].clone();
                    let meta = PendingTrackDeletionMeta {
                        track_name: track.name.clone(),
                        port: track.port,
                        channel: track.channel,
                        original_index: idx,
                    };

                    // 仅释放 UI 入口：从 sidebar.tracks 中移除。
                    // 内存数据 / 磁盘缓存由 Root → Runner 异步写入 `.lmdeltrack`。
                    // 保留 track_id 为占用状态（reserved_track_ids），
                    // 新建音轨时跳过该 ID，避免编号复用导致选中冲突。
                    self.tracks.remove(idx);
                    self.mark_track_id_reserved(id);
                    self.pending_track_deletion = Some(id);
                    self.pending_track_deletion_meta = Some(meta);
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

    /// 处理打开音轨列表面板空白区域右键菜单
    pub(super) fn handle_panel_context_menu_opened(&mut self) {
        self.panel_context_menu.is_open = true;
        // 关闭其他浮动菜单，避免叠加
        self.track_context_menu = TrackContextMenuState::default();
        self.color_picking_track = None;
    }

    /// 处理关闭音轨列表面板空白区域右键菜单
    pub(super) fn handle_panel_context_menu_closed(&mut self) {
        self.panel_context_menu.is_open = false;
    }

    /// 处理点击音轨列表面板空白区域右键菜单项
    pub(super) fn handle_panel_context_menu_item_clicked(&mut self, item: PanelContextMenuItem) {
        self.panel_context_menu.is_open = false;
        match item {
            PanelContextMenuItem::RecoverDeletedTrack => {
                // 请求 Root 转发给 Runner 打开"找回删除音轨"对话框。
                // 对话框需要列出缓存目录下所有 `.lmdeltrack` 文件，
                // Runner 在打开前会扫描缓存目录并填充对话框状态。
                self.pending_recover_track_dialog = true;
            }
        }
    }
}
