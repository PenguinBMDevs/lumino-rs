//! 音轨通道选择处理 — 打开选择器、选择通道、关闭
//!
//! 通道内部值 `0..=15`（MIDI 标准），UI 显示 `1..=16`（Domino/DAW 惯例）。
//! 与端口编辑不同：通道是**内容属性**（音符/控制事件/自动化 lane 均携带），
//! sidebar 只负责 UI 侧即时更新与重排，真正的数据改写由 Root 消费 pending 完成。

use crate::sidebar::core::{Sidebar, TrackContextMenuState};

impl Sidebar {
    /// 处理打开通道选择器
    pub(super) fn handle_track_channel_picker_opened(&mut self, id: usize) {
        self.channel_picking_track = Some(id);
        self.track_context_menu = TrackContextMenuState::default();
        // 互斥：关闭面板空白菜单，避免选完通道后残留「找回删除音轨」浮层。
        self.panel_context_menu.reset();
        self.renaming_track = None;
        self.color_picking_track = None;
        self.port_picking_track = None;
    }

    /// 处理选择音轨通道（内部值，UI 显示值为其 +1）。
    ///
    /// 通道未变（点中当前值）→ 无操作，不置脏、不重排、不产生 pending；
    /// 命中后同步标签并按 `(port, channel, id)` 重排，写入
    /// `(id, 旧通道, 新通道)` 供 Root 批量改写音符/控制事件与失败回滚。
    pub(super) fn handle_track_channel_selected(&mut self, id: usize, channel: u8) {
        self.channel_picking_track = None;
        // 选择完成时再兜底关闭面板空白菜单（防右键穿透置位后残留浮层）。
        self.panel_context_menu.reset();

        // 防御性收口：通道上限 0..=15（16 通道），越界输入夹紧。
        let channel = channel.min(Sidebar::CHANNEL_CHOICES - 1);

        let Some(track) = self.tracks.iter_mut().find(|t| t.id == id) else {
            return;
        };
        // 指挥轨不发音符、无通道语义；点中当前通道视为无操作。
        if track.is_conductor || track.channel == channel {
            return;
        }
        let old_channel = track.channel;
        track.channel = channel;
        track.display_label = Self::track_label(track.port, channel);

        // 重排前清理拖拽状态（hover_index 基于旧行序坐标）。
        self.track_reorder = None;

        // 与 `update_tracks_from_midi` 同口径：port → channel → id。
        self.tracks.sort_by_key(|t| (t.port, t.channel, t.id));

        self.pending_track_channel_change = Some((id, old_channel, channel));
    }

    /// 处理关闭通道选择器
    pub(super) fn handle_track_channel_picker_closed(&mut self, _id: usize) {
        self.channel_picking_track = None;
    }
}
