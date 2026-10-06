//! 音轨端口选择处理 — 打开选择器、选择端口、关闭
//!
//! 端口内部值 `0..=15`（产品上限 16 端口），UI 显示 `1..=16`
//! （对齐 Domino 与各类 DAW 的端口编号惯例）。

use crate::sidebar::core::{Sidebar, TrackContextMenuState};

impl Sidebar {
    /// 处理打开端口选择器
    pub(super) fn handle_track_port_picker_opened(&mut self, id: usize) {
        self.port_picking_track = Some(id);
        self.track_context_menu = TrackContextMenuState::default();
        // 互斥：关闭面板空白菜单，避免选完端口后残留「找回删除音轨」浮层。
        self.panel_context_menu.reset();
        self.renaming_track = None;
        self.color_picking_track = None;
        self.channel_picking_track = None;
    }

    /// 处理选择音轨端口（内部值，UI 显示值为其 +1）。
    ///
    /// 语义：
    /// - 端口未变（点中当前值）→ 无操作，不置脏、不重排、不产生 pending；
    /// - 命中后同步标签与 `(port, channel, id)` 重排（与导入口径一致），
    ///   并写入 `(id, 旧端口, 新端口)` 供 Root 写回文档（回滚也依赖旧端口）。
    pub(super) fn handle_track_port_selected(&mut self, id: usize, port: u8) {
        self.port_picking_track = None;
        // 选择完成时再兜底关闭面板空白菜单（防右键穿透置位后残留浮层）。
        self.panel_context_menu.reset();

        // 防御性收口：端口上限 0..=15（产品 16 端口），越界输入夹紧。
        let port = port.min(Sidebar::PORT_CHOICES - 1);

        let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) else {
            return;
        };
        // 指挥轨不发音符、无端口语义；点中当前端口视为无操作（不置工程脏）。
        if track.is_conductor || track.port == port {
            return;
        }
        let old_port = track.port;
        track.port = port;
        track.display_label = Self::track_label(port, track.channel);

        // 重排前清理拖拽状态：`hover_index` 基于旧行序坐标，重排后不失效会指错位置。
        self.track_reorder = None;

        // 与 `update_tracks_from_midi` 同口径：port → channel → id。
        // 编辑端口会改变排序键，必须重排以保持列表与数据一致。
        self.tracks.sort_by_key(|t| (t.port, t.channel, t.id));

        self.pending_track_port_change = Some((id, old_port, port));
    }

    /// 处理关闭端口选择器
    pub(super) fn handle_track_port_picker_closed(&mut self, _id: usize) {
        self.port_picking_track = None;
    }
}
